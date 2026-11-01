//! Rust visibility: an item's written visibility, and its inherent
//! container's, placed as the module territory, crate or dependents it
//! reaches. Placement reads the module layout (`x.rs` owns `x/`; a crate
//! root and `mod.rs` own their directory); an inline `mod` is read as its
//! file, which only widens a bound, never narrows it.
use octocode_engine::graph::{GraphFactDeclaration, GraphFactsDocument};
use std::path::{Path, PathBuf};

/// Declarations a method's type may be.
const TYPE_KINDS: [&str; 3] = ["struct", "enum", "type"];

/// A visibility as written, before it is placed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Written {
    /// Private or `pub(self)`: the declaring module.
    Module,
    /// `pub(super)`, `pub(in super::super)`: so many modules up.
    Up(usize),
    /// `pub(in crate::a::b)`: the ancestor module these segments name.
    Named(Vec<String>),
    /// `pub(crate)`, or a restriction this reading cannot place.
    Crate,
    Public,
}

/// Where a Rust item is visible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Bound {
    /// Within one module: the files under this directory and the file of
    /// the same name (`a/b/` and `a/b.rs`).
    Territory(PathBuf),
    /// Within its crate.
    Crate,
    /// In its crate and every crate that depends on it.
    Public,
}

/// The visibilities that bound a declaration: its own and its container's.
pub(super) struct Declared {
    visibilities: Vec<Written>,
    /// The narrowest as written (`pub(super)`), for the reader.
    pub(super) written: String,
}

impl Declared {
    /// The narrowest bound of the declaration in `file`, of the package at
    /// `package` whose target root files are `roots`.
    pub(super) fn bound(&self, file: &Path, package: &Path, roots: &[PathBuf]) -> Bound {
        self.visibilities
            .iter()
            .map(|written| place(written, file, package, roots))
            .reduce(narrower)
            .unwrap_or(Bound::Public)
    }
}

/// The declaration of `symbol` whose name holds zero-based `at` in
/// `document`, with what bounds it; `None` when none does, or for an item
/// whose reach its own text does not state: a trait or trait-impl member
/// (as visible as the trait), a macro (textual and exported scope), an
/// `impl` block.
pub(super) fn declared(
    document: &GraphFactsDocument,
    content: &str,
    at: (u32, u32),
    symbol: &str,
) -> Option<Declared> {
    let declaration = document.declarations.iter().find(|declaration| {
        declaration.name == symbol
            && (
                declaration.selection_range.start.line,
                declaration.selection_range.start.character,
            ) <= at
            && at
                <= (
                    declaration.selection_range.end.line,
                    declaration.selection_range.end.character,
                )
    })?;
    if matches!(declaration.kind.as_str(), "macro" | "impl") {
        return None;
    }
    let own = header(content, declaration)?;
    let mut found = vec![(written(own), own)];
    let parent = declaration
        .parent
        .as_deref()
        .and_then(|id| document.declarations.iter().find(|parent| parent.id == id));
    match parent.map(|parent| (parent.kind.as_str(), parent)) {
        Some(("trait", _)) => return None,
        Some(("impl", block)) => {
            let start = usize::try_from(block.source_range?.range.start_byte).ok()?;
            let head = content.get(start..)?.split('{').next()?;
            if implements_trait(head) {
                return None;
            }
            if let Some(text) = document
                .declarations
                .iter()
                .find(|item| item.name == block.name && TYPE_KINDS.contains(&item.kind.as_str()))
                .and_then(|item| header(content, item))
            {
                found.push((written(text), text));
            }
        }
        _ => {}
    }
    let shown = found
        .iter()
        .map(|(_, text)| visibility_text(text))
        .collect::<Vec<_>>()
        .join(" (on a ")
        + if found.len() > 1 { " type)" } else { "" };
    Some(Declared {
        visibilities: found.into_iter().map(|(written, _)| written).collect(),
        written: shown,
    })
}

/// The text from a declaration's start to its name (`pub(super) fn `), of
/// the file's own source unit.
fn header<'a>(content: &'a str, declaration: &GraphFactDeclaration) -> Option<&'a str> {
    let whole = declaration.source_range.filter(|span| span.unit.0 == 0)?;
    let name = declaration
        .selection_byte_range
        .filter(|span| span.unit.0 == 0)?;
    content.get(
        usize::try_from(whole.range.start_byte).ok()?
            ..usize::try_from(name.range.start_byte).ok()?,
    )
}

/// `header` without leading attributes and comments.
fn code(header: &str) -> &str {
    let mut rest = header.trim_start();
    loop {
        if rest.starts_with("#[") || rest.starts_with("#![") {
            let mut depth = 0usize;
            let end = rest.char_indices().find_map(|(at, c)| {
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(at + 1);
                        }
                    }
                    _ => {}
                }
                None
            });
            rest = rest
                .get(end.unwrap_or(rest.len())..)
                .unwrap_or("")
                .trim_start();
        } else if rest.starts_with("//") {
            rest = rest
                .split_once('\n')
                .map_or("", |(_, after)| after)
                .trim_start();
        } else if rest.starts_with("/*") {
            rest = rest
                .split_once("*/")
                .map_or("", |(_, after)| after)
                .trim_start();
        } else {
            return rest;
        }
    }
}

/// The visibility `header` writes, in its canonical spelling
/// (`pub(super)`), or `private`.
fn visibility_text(header: &str) -> String {
    match written(header) {
        Written::Module if code(header).starts_with("pub") => "pub(self)".to_owned(),
        Written::Module => "private".to_owned(),
        Written::Up(1) => "pub(super)".to_owned(),
        Written::Up(steps) => format!("pub(in {})", vec!["super"; steps].join("::")),
        Written::Named(segments) => format!("pub(in crate::{})", segments.join("::")),
        Written::Crate => "pub(crate)".to_owned(),
        Written::Public => "pub".to_owned(),
    }
}

fn written(header: &str) -> Written {
    let rest = code(header);
    let Some(after) = rest.strip_prefix("pub") else {
        return Written::Module;
    };
    if after.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return Written::Module;
    }
    let Some(scoped) = after.trim_start().strip_prefix('(') else {
        return Written::Public;
    };
    let Some((inner, _)) = scoped.split_once(')') else {
        return Written::Crate;
    };
    let inner = inner.trim();
    let path = inner.strip_prefix("in ").map_or(inner, str::trim);
    let segments = path.split("::").map(str::trim).collect::<Vec<_>>();
    match segments.as_slice() {
        ["crate"] => Written::Crate,
        ["self"] => Written::Module,
        ["crate", named @ ..] => Written::Named(named.iter().map(|s| (*s).to_owned()).collect()),
        relative if relative.iter().all(|s| matches!(*s, "self" | "super")) => {
            Written::Up(relative.iter().filter(|s| **s == "super").count())
        }
        _ => Written::Crate,
    }
}

/// Whether an `impl` head (up to its `{`) implements a trait: `for` as a
/// keyword, not a higher-ranked `for<'a>` bound.
fn implements_trait(head: &str) -> bool {
    head.match_indices("for").any(|(at, _)| {
        let before = head[..at].chars().next_back();
        let after = &head[at + "for".len()..];
        before.is_some_and(char::is_whitespace)
            && after.starts_with(char::is_whitespace)
            && !after.trim_start().starts_with('<')
    })
}

/// The directory of the module `file` defines: its own for a crate root or
/// `mod.rs`, else the one named by its stem.
fn module_dir(file: &Path, roots: &[PathBuf]) -> PathBuf {
    let parent = file.parent().unwrap_or(file);
    let named = file.file_name().and_then(|name| name.to_str());
    let container = parent.file_name().and_then(|name| name.to_str());
    let root = matches!(named, Some("lib.rs" | "main.rs" | "mod.rs" | "build.rs"))
        || matches!(container, Some("tests" | "benches" | "examples" | "bin"))
        || roots.iter().any(|root| root == file);
    if root {
        parent.to_path_buf()
    } else {
        file.with_extension("")
    }
}

fn place(written: &Written, file: &Path, package: &Path, roots: &[PathBuf]) -> Bound {
    let module = module_dir(file, roots);
    let within = |dir: &Path| {
        dir.starts_with(package)
            .then(|| Bound::Territory(dir.to_path_buf()))
    };
    match written {
        Written::Module => within(&module),
        Written::Up(steps) => module.ancestors().nth(*steps).and_then(within),
        // The outermost match: a repeated directory name widens, never
        // narrows, the bound.
        Written::Named(segments) => module
            .ancestors()
            .filter(|dir| {
                let tail = dir.components().rev().take(segments.len());
                tail.map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .eq(segments.iter().rev().cloned())
            })
            .last()
            .and_then(within),
        Written::Crate => Some(Bound::Crate),
        Written::Public => Some(Bound::Public),
    }
    .unwrap_or(Bound::Crate)
}

fn narrower(left: Bound, right: Bound) -> Bound {
    match (left, right) {
        (Bound::Territory(a), Bound::Territory(b)) => {
            Bound::Territory(if b.starts_with(&a) { b } else { a })
        }
        (territory @ Bound::Territory(_), _) | (_, territory @ Bound::Territory(_)) => territory,
        (Bound::Crate, _) | (_, Bound::Crate) => Bound::Crate,
        _ => Bound::Public,
    }
}

/// Whether `file` lies in the module whose directory is `territory`.
pub(super) fn in_territory(file: &Path, territory: &Path) -> bool {
    file.starts_with(territory) || file == territory.with_extension("rs")
}

/// Whether `file` is the module file `target` or one of its child modules.
pub(super) fn in_module(file: &Path, target: &Path) -> bool {
    let own = if target.file_name().is_some_and(|name| name == "mod.rs") {
        target.parent().unwrap_or(target).to_path_buf()
    } else {
        target.with_extension("")
    };
    file == target || file.starts_with(own)
}

#[cfg(test)]
mod tests;
