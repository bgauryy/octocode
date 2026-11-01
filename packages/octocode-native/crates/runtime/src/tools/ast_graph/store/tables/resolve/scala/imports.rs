//! Scala imports: the clauses as written, read into the names they bring
//! in, and what they bind at a call site.
//!
//! An import is visible only inside the declaration whose body holds it
//! (file-level ones everywhere). A wildcard over a path the scan does not
//! hold may supply any name: it outranks a package member of another file,
//! so a name it could supply links nothing.
use super::super::linker::{Linked, Linker, Site};
use super::super::*;
use super::linearize::Lookup;
use super::scope::{unique, upper};

/// What a wildcard import's path names.
enum Wildcard {
    /// An in-repo object: its members.
    Object(u32, Confidence),
    /// A scanned package: its top-level declarations.
    Package,
    /// A path outside the scan, or an object whose members are not all known.
    Unknown,
}

impl Linker<'_> {
    /// The imports visible at `site`: file-level ones, and those of a
    /// declaration that encloses the caller. An import whose declaration is
    /// not in the graph counts as visible.
    fn visible_imports<'s>(&self, site: &Site<'s>) -> Vec<Imported<'s>> {
        let specs = site
            .bindings
            .written_imports
            .iter()
            .filter(|import| {
                import.scope.is_none_or(|id| {
                    site.index
                        .decl_nodes
                        .get(id)
                        .is_none_or(|node| self.chain(Some(site.caller)).any(|c| c == *node))
                })
            })
            .map(|import| import.specifier)
            .collect::<Vec<_>>();
        imported(&specs)
    }

    /// What an import brings in under `site.name`: an explicit selector, or
    /// a wildcard over an in-repo object or package (`import Outer._`,
    /// `import cats.data.Chain.{one, Wrap}`). `Err` when an explicit import
    /// names no in-repo member, two imports disagree, or a wildcard outside
    /// the scan could supply the name.
    pub(super) fn scala_imported(&self, site: &Site) -> Result<Option<Linked>, ()> {
        let all = self.visible_imports(site);
        let explicit = all
            .iter()
            .filter(|import| !import.wildcard() && import.local == site.name)
            .collect::<Vec<_>>();
        let mut found = Vec::<(Vec<u32>, Confidence)>::new();
        let mut add = |targets: Vec<u32>, confidence: Confidence| {
            if !targets.is_empty() && !found.iter().any(|(seen, _)| *seen == targets) {
                found.push((targets, confidence));
            }
        };
        let mut unknown = false;
        if explicit.is_empty() {
            for import in all.iter().filter(|import| import.wildcard()) {
                match self.wildcard(site, &import.path) {
                    Wildcard::Object(module, confidence) => {
                        match self.linearized(module, site.name, site.accept, false) {
                            Lookup::Hit(targets, weakest, _) => {
                                add(targets, confidence.max(weakest));
                            }
                            Lookup::Blocked => unknown = true,
                            Lookup::Clear => {}
                        }
                    }
                    Wildcard::Package => {
                        // Same-named build variants stay ambiguous.
                        match self.packaged(site, &import.path, site.name, site.accept)[..] {
                            [] => {}
                            [only] => add(vec![only], Confidence::High),
                            _ => unknown = true,
                        }
                    }
                    Wildcard::Unknown => unknown = true,
                }
            }
        }
        for import in &explicit {
            let Some((module, confidence)) = self.import_object(site, &import.path) else {
                // `import cats.data.NonEmptyList`: a package's top-level name.
                if !import.path.iter().any(|s| upper(s)) {
                    add(
                        self.packaged(site, &import.path, import.name, site.accept),
                        Confidence::High,
                    );
                }
                continue;
            };
            if let Lookup::Hit(targets, weakest, _) =
                self.linearized(module, import.name, site.accept, false)
            {
                add(targets, confidence.max(weakest));
            }
        }
        match found.as_slice() {
            [(targets, confidence)] if !unknown => {
                let confidence = confidence_for(targets, *confidence);
                Ok(Some((targets.clone(), "import", confidence)))
            }
            [] if explicit.is_empty() && !unknown => Ok(None),
            _ => Err(()),
        }
    }

    /// What a wildcard import's path names: an in-repo object (`Outer`,
    /// `cats.data.Chain`, the lower-case `cats.syntax.all`), else a scanned
    /// package (`cats.data`).
    fn wildcard(&self, site: &Site, path: &[&str]) -> Wildcard {
        if let Some((module, confidence)) = self.import_object(site, path) {
            return Wildcard::Object(module, confidence);
        }
        if path.iter().any(|s| upper(s)) {
            return Wildcard::Unknown;
        }
        if let Some((last, package)) = path.split_last() {
            match self.packaged(site, package, last, &|id| self.is_module(id))[..] {
                [] => {}
                [module] => return Wildcard::Object(module, Confidence::High),
                _ => return Wildcard::Unknown,
            }
        }
        if self.scanned_package(path) {
            Wildcard::Package
        } else {
            Wildcard::Unknown
        }
    }

    /// Whether a lower-case path (`cats.data`) names the directory of a
    /// scanned Scala file, by suffix.
    fn scanned_package(&self, path: &[&str]) -> bool {
        let package = path.join("/");
        let memo = &self.bases.linearizations.packages;
        if let Some(known) = memo.borrow().get(&package) {
            return *known;
        }
        let suffix = format!("/{package}");
        let scanned = self.scope.file_paths.iter().any(|file| {
            let dir = crate::tools::ast_graph::paths::parent(file);
            [".scala", ".sc", ".sbt"]
                .iter()
                .any(|ext| file.ends_with(ext))
                && (dir == package || dir.ends_with(&suffix))
        });
        memo.borrow_mut().insert(package, scanned);
        scanned
    }

    /// The in-repo object an import path names: one of this file, else as
    /// a term path from the caller.
    fn import_object(&self, site: &Site, path: &[&str]) -> Option<(u32, Confidence)> {
        if let Some(module) = self.file_object_path(site, path) {
            return Some((module, Confidence::High));
        }
        self.scala_object(site, &path.join("."))
    }

    /// A top-level declaration named `name` in another Scala file: the one
    /// an explicit import names (an import naming no match is foreign),
    /// else the only one in the caller's directory, else the only one. A
    /// `term` (an object) that a wildcard outside the scan could supply links
    /// nothing; a type name keeps the directory rule, since blocking it would
    /// drop most receiver-type and base links of a partial scan, but only at
    /// medium confidence: the wildcard outranks the package member.
    pub(super) fn imported_or_near(
        &self,
        site: &Site,
        name: &str,
        kind: &dyn Fn(u32) -> bool,
        term: bool,
    ) -> Option<(u32, Confidence, &'static str)> {
        let visible = self.visible_imports(site);
        let explicit = visible
            .iter()
            .filter(|import| !import.wildcard() && import.local == name)
            .collect::<Vec<_>>();
        if !explicit.is_empty() {
            let mut found = Vec::new();
            for import in &explicit {
                found.extend(self.packaged(site, &import.path, import.name, kind));
            }
            found.sort_unstable();
            found.dedup();
            return Some((unique(&found)?, Confidence::High, "import"));
        }
        let unknown = visible.iter().any(|import| {
            import.wildcard() && matches!(self.wildcard(site, &import.path), Wildcard::Unknown)
        });
        if term && unknown {
            return None;
        }
        let caller_file = self.file_of(site.caller);
        let pool = self
            .named(site.caller, name)
            .filter(|id| {
                kind(*id)
                    && self.nodes[*id as usize].parent == NONE
                    && self.file_of(*id) != caller_file
                    && self.scala_node(*id)
            })
            .collect::<Vec<_>>();
        let caller_dir = self.dir_of(caller_file);
        let near = pool
            .iter()
            .copied()
            .filter(|id| self.dir_of(self.file_of(*id)) == caller_dir)
            .collect::<Vec<_>>();
        if !near.is_empty() {
            let confidence = if unknown {
                Confidence::Medium
            } else {
                Confidence::High
            };
            return Some((unique(&near)?, confidence, "same-package"));
        }
        Some((unique(&pool)?, Confidence::Medium, "unique-name"))
    }

    /// Declarations named `name` that a path (`a.b`, `a.b.Outer`) reaches:
    /// the leading lower-case segments name the package, by directory
    /// suffix; the rest name enclosing objects.
    pub(super) fn packaged(
        &self,
        site: &Site,
        path: &[&str],
        name: &str,
        kind: &dyn Fn(u32) -> bool,
    ) -> Vec<u32> {
        let objects = path.iter().position(|s| upper(s)).unwrap_or(path.len());
        let (package, outer) = path.split_at(objects);
        let package = package.join("/");
        self.named(site.caller, name)
            .filter(|id| kind(*id) && self.scala_node(*id))
            .filter(|id| {
                let mut parents = Vec::new();
                let mut parent = self.nodes[*id as usize].parent;
                while parent != NONE {
                    parents.push(parent);
                    parent = self.nodes[parent as usize].parent;
                }
                parents.reverse();
                let names = parents
                    .iter()
                    .map(|p| self.strings[self.nodes[*p as usize].name as usize].as_str())
                    .collect::<Vec<_>>();
                let dir = self.dir_of(self.file_of(*id));
                names == outer
                    && parents.iter().all(|p| self.is_module(*p))
                    && (package.is_empty()
                        || dir == package
                        || dir.ends_with(&format!("/{package}")))
            })
            .collect()
    }
}

/// One imported name: the path before it, the member it names (`_` for a
/// wildcard) and its local name.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct Imported<'a> {
    pub path: Vec<&'a str>,
    pub name: &'a str,
    pub local: &'a str,
}

impl Imported<'_> {
    pub(super) fn wildcard(&self) -> bool {
        self.name == "_"
    }
}

/// The names `specs` import: `a.b.C`, `a.b._`, `a.b.*`, `a.{B, C => D, _}`,
/// `a.{B as D}` and several comma-separated clauses. A hidden name
/// (`C => _`) and `given` selectors import nothing by name.
pub(super) fn imported<'a>(specs: &[&'a str]) -> Vec<Imported<'a>> {
    let mut out = Vec::new();
    for spec in specs {
        for clause in top_level_clauses(spec) {
            let clause = clause.trim();
            if let Some((path, selectors)) = clause.split_once('{') {
                let path = segments(path.trim().trim_end_matches('.'));
                for selector in selectors.trim_end_matches('}').split(',') {
                    push(&mut out, &path, selector.trim());
                }
            } else if let Some((path, last)) = clause.rsplit_once('.') {
                push(&mut out, &segments(path), last.trim());
            }
        }
    }
    out
}

fn push<'a>(out: &mut Vec<Imported<'a>>, path: &[&'a str], selector: &'a str) {
    if path.is_empty() || selector.is_empty() || selector.starts_with("given") {
        return;
    }
    let (name, local) = match selector
        .split_once("=>")
        .or_else(|| selector.split_once(" as "))
    {
        Some((name, local)) => (name.trim(), local.trim()),
        None => (selector, selector),
    };
    if local == "_" && name != "_" {
        return;
    }
    let wildcard = matches!(name, "_" | "*");
    out.push(Imported {
        path: path.to_vec(),
        name: if wildcard { "_" } else { name },
        local: if wildcard { "_" } else { local },
    });
}

fn segments(path: &str) -> Vec<&str> {
    path.split('.').map(str::trim).collect()
}

/// `a.b, c.{d, e}` → `a.b`, `c.{d, e}`.
fn top_level_clauses(spec: &str) -> Vec<&str> {
    let mut clauses = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (at, c) in spec.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                clauses.push(&spec[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    clauses.push(&spec[start..]);
    clauses
}

#[cfg(test)]
mod tests {
    use super::{Imported, imported};

    fn row<'a>(path: &[&'a str], name: &'a str, local: &'a str) -> Imported<'a> {
        Imported {
            path: path.to_vec(),
            name,
            local,
        }
    }

    #[test]
    fn import_clauses_name_each_binding() {
        assert_eq!(
            imported(&[
                "cats.data.ContT",
                "cats.syntax.all._",
                "a.{B, C => D, E => _, given, _}",
                "x.y.*, p.{Q as R}",
            ]),
            [
                row(&["cats", "data"], "ContT", "ContT"),
                row(&["cats", "syntax", "all"], "_", "_"),
                row(&["a"], "B", "B"),
                row(&["a"], "C", "D"),
                row(&["a"], "_", "_"),
                row(&["x", "y"], "_", "_"),
                row(&["p"], "Q", "R"),
            ]
        );
    }
}
