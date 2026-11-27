//! Java and C# type names: what a simple or qualified type name binds from
//! a scope, and a C# type's namespace and partial parts.
//!
//! A simple name binds, first match wins: a type an enclosing declaration
//! (or the file) declares; a single-type import (Java) or `using` alias
//! (C#); a top-level type of the caller's package directory (Java) or of
//! the caller's namespace or an enclosing one (C#); an on-demand import
//! (`import a.b.*;`, `using A.B;`). A qualified name binds its head, then
//! nested types; else a package (Java: directory suffix) or namespace path.
//! A type a scan cannot see binds nothing: no graph-wide name match.
use super::super::linker::{Linker, Site};
use super::super::*;
use super::ClassLang;
use std::cell::RefCell;

/// File node → its C# file-scoped namespace, memoized.
#[derive(Default)]
pub(crate) struct Namespaces(RefCell<HashMap<u32, Option<String>>>);

/// A bound type (C#: every partial part), how it bound, and how sure.
pub(super) type Bound = (Vec<u32>, &'static str, Confidence);

fn identifier(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '@'))
}

impl Linker<'_> {
    /// A type declaration, also one whose own extent was recovered: it is
    /// no target, but its intact members are.
    pub(super) fn class_like(&self, id: u32) -> bool {
        (self.is_type)(id)
            || octocode_engine::signatures::TYPE_DECLARATION_KINDS.contains(&self.symbol_kind(id))
    }

    /// The type `written` names from the scopes enclosing `start`.
    pub(super) fn class_type(
        &self,
        site: &Site,
        lang: ClassLang,
        written: &str,
        start: Option<u32>,
    ) -> Option<Bound> {
        let written = written.strip_prefix("global::").unwrap_or(written);
        let segments = written.split('.').collect::<Vec<_>>();
        if !segments.iter().all(|s| identifier(s)) {
            return None;
        }
        let (first, rest) = segments.split_first()?;
        match self.simple_type(site, lang, first, start) {
            Some((mut owners, via, confidence)) => {
                for name in rest {
                    owners = self.nested_type(lang, &owners, name)?;
                }
                Some((owners, via, confidence))
            }
            None if rest.is_empty() => None,
            None => self
                .qualified_type(site, lang, &segments)
                .map(|owners| (owners, "import", Confidence::High)),
        }
    }

    fn simple_type(
        &self,
        site: &Site,
        lang: ClassLang,
        name: &str,
        start: Option<u32>,
    ) -> Option<Bound> {
        for scope in self.chain(start).map(Some).chain([None]) {
            let anchor = scope.unwrap_or(site.caller);
            let types = self.types_in(scope, anchor, name);
            if !types.is_empty() {
                return Some((self.one_type(lang, types)?, "local", Confidence::High));
            }
        }
        let imports = &site.bindings.written_imports;
        // A single-type import or alias shadows the package; one naming a
        // type outside the scan binds nothing.
        let single = imports.iter().find(|import| match lang {
            ClassLang::Java => {
                !import.specifier.ends_with(".*")
                    && import.specifier.rsplit('.').next() == Some(name)
            }
            ClassLang::CSharp => import.alias == Some(name),
        });
        if let Some(import) = single {
            let path = import.specifier.trim_start_matches("static ").trim();
            let segments = path.split('.').collect::<Vec<_>>();
            let owners = self.qualified_type(site, lang, &segments)?;
            return Some((owners, "import", Confidence::High));
        }
        let package = self.package_types(site, lang, name);
        if !package.is_empty() {
            let owners = self.one_type(lang, package)?;
            return Some((owners, "same-package", lang.package_confidence()));
        }
        let mut found = Vec::new();
        for import in imports {
            let path = match lang {
                ClassLang::Java if !import.specifier.starts_with("static ") => {
                    import.specifier.strip_suffix(".*")
                }
                ClassLang::CSharp if import.alias.is_none() => Some(import.specifier),
                _ => None,
            };
            let Some(path) = path else { continue };
            let segments = path.split('.').collect::<Vec<_>>();
            found.extend(self.in_package(site, lang, &segments, name));
        }
        found.sort_unstable();
        found.dedup();
        if !found.is_empty() {
            let owners = self.one_type(lang, found)?;
            return Some((owners, "import-glob", lang.package_confidence()));
        }
        self.java_unique_type(site, lang, name)
    }

    /// A package spread over several source roots (`src/main/java/a`,
    /// `src/test/java/a`) is no one directory: the only Java type of that
    /// name, at low confidence, unless an on-demand import could supply it.
    fn java_unique_type(&self, site: &Site, lang: ClassLang, name: &str) -> Option<Bound> {
        let on_demand = site.bindings.written_imports.iter().any(|import| {
            !import.specifier.starts_with("static ") && import.specifier.ends_with(".*")
        });
        if lang != ClassLang::Java || on_demand || name.len() < MIN_UNIQUE_NAME_LEN {
            return None;
        }
        let caller_file = self.file_of(site.caller);
        let pool = self
            .top_types(site, lang, name)
            .filter(|id| self.file_of(*id) != caller_file)
            .collect::<Vec<_>>();
        match pool.as_slice() {
            [only] => Some((vec![*only], "unique-name", Confidence::Low)),
            _ => None,
        }
    }

    /// Types `scope` (the file's top level for `None`) declares as `name`;
    /// for a C# type, in any of its partial parts.
    fn types_in(&self, scope: Option<u32>, anchor: u32, name: &str) -> Vec<u32> {
        let parts = match scope {
            Some(id) if self.class_like(id) => self.class_parts(id),
            Some(id) => vec![id],
            None => {
                return self
                    .declared_by(None, anchor, name)
                    .into_iter()
                    .filter(|id| self.class_like(*id))
                    .collect();
            }
        };
        let mut types = parts
            .iter()
            .flat_map(|part| self.declared_by(Some(*part), *part, name))
            .filter(|id| self.class_like(*id))
            .collect::<Vec<_>>();
        types.sort_unstable();
        types.dedup();
        types
    }

    fn nested_type(&self, lang: ClassLang, owners: &[u32], name: &str) -> Option<Vec<u32>> {
        let mut types = owners
            .iter()
            .flat_map(|owner| self.types_in(Some(*owner), *owner, name))
            .collect::<Vec<_>>();
        types.sort_unstable();
        types.dedup();
        self.one_type(lang, types)
    }

    /// One type: Java a unique declaration, C# the parts of one type.
    fn one_type(&self, lang: ClassLang, ids: Vec<u32>) -> Option<Vec<u32>> {
        match (lang, ids.as_slice()) {
            (_, []) => None,
            (ClassLang::Java, [only]) => Some(vec![*only]),
            (ClassLang::Java, _) => None,
            (ClassLang::CSharp, [first, rest @ ..]) => {
                let name = self.qualified(*first);
                rest.iter()
                    .all(|id| self.qualified(*id) == name)
                    .then(|| self.class_parts(*first))
            }
        }
    }

    /// Top-level types named `name` of the caller's package (Java: its
    /// directory) or namespace, innermost namespace first (C#).
    fn package_types(&self, site: &Site, lang: ClassLang, name: &str) -> Vec<u32> {
        let caller_file = self.file_of(site.caller);
        match lang {
            ClassLang::Java => {
                let dir = self.dir_of(caller_file);
                self.top_types(site, lang, name)
                    .filter(|id| self.dir_of(self.file_of(*id)) == dir)
                    .collect()
            }
            ClassLang::CSharp => {
                let mut namespace = self.namespace_of(site.caller);
                loop {
                    let types = self
                        .top_types(site, lang, name)
                        .filter(|id| self.namespace_of(*id) == namespace)
                        .collect::<Vec<_>>();
                    if !types.is_empty() {
                        return types;
                    }
                    let Some((outer, _)) = namespace.rsplit_once('.') else {
                        break;
                    };
                    namespace = outer.to_owned();
                }
                if namespace.is_empty() {
                    return Vec::new();
                }
                self.top_types(site, lang, name)
                    .filter(|id| self.namespace_of(*id).is_empty())
                    .collect()
            }
        }
    }

    /// Top-level (C#: namespace-level) types of the caller's language.
    fn top_types<'s>(
        &'s self,
        site: &Site,
        lang: ClassLang,
        name: &str,
    ) -> impl Iterator<Item = u32> + 's {
        self.named(site.caller, name).filter(move |id| {
            let parent = self.nodes[*id as usize].parent;
            self.class_like(*id)
                && self.class_file(lang, *id)
                && (parent == NONE
                    || lang == ClassLang::CSharp && self.symbol_kind(parent) == "namespace")
        })
    }

    pub(super) fn class_file(&self, lang: ClassLang, id: u32) -> bool {
        self.scope
            .path(self.file_of(id))
            .is_some_and(|path| path.ends_with(lang.extension()))
    }

    /// `a.b.C.D` as a package (Java: directory suffix) or namespace path,
    /// then nested types.
    pub(super) fn qualified_type(
        &self,
        site: &Site,
        lang: ClassLang,
        segments: &[&str],
    ) -> Option<Vec<u32>> {
        for split in (1..segments.len()).rev() {
            let (package, types) = segments.split_at(split);
            let Some((first, nested)) = types.split_first() else {
                continue;
            };
            let found = self.in_package(site, lang, package, first);
            if found.is_empty() {
                continue;
            }
            let mut owners = self.one_type(lang, found)?;
            for name in nested {
                owners = self.nested_type(lang, &owners, name)?;
            }
            return Some(owners);
        }
        None
    }

    /// Top-level types named `name` in the package `path`.
    fn in_package(&self, site: &Site, lang: ClassLang, path: &[&str], name: &str) -> Vec<u32> {
        let package = path.join(lang.package_separator());
        self.top_types(site, lang, name)
            .filter(|id| self.package_is(lang, *id, &package))
            .collect()
    }

    fn package_is(&self, lang: ClassLang, id: u32, package: &str) -> bool {
        match lang {
            ClassLang::Java => {
                let dir = self.dir_of(self.file_of(id));
                dir == package || dir.ends_with(&format!("/{package}"))
            }
            ClassLang::CSharp => self.namespace_of(id) == package,
        }
    }

    /// Every partial part of a C# type (the type alone otherwise): the
    /// types sharing its qualified name.
    pub(super) fn class_parts(&self, id: u32) -> Vec<u32> {
        if !self.class_file(ClassLang::CSharp, id) {
            return vec![id];
        }
        let name = &self.strings[self.nodes[id as usize].name as usize];
        let qualified = self.qualified(id);
        let mut parts = self
            .named(id, name)
            .filter(|other| {
                *other == id
                    || self.class_like(*other)
                        && self.class_file(ClassLang::CSharp, *other)
                        && self.qualified(*other) == qualified
            })
            .collect::<Vec<_>>();
        if parts.is_empty() {
            parts.push(id);
        }
        parts
    }

    /// A C# declaration's namespace and enclosing types, dotted.
    fn qualified(&self, id: u32) -> String {
        let mut names = Vec::new();
        let mut current = id;
        while current != NONE {
            let node = &self.nodes[current as usize];
            if node.kind != NodeKind::Symbol {
                break;
            }
            names.push(self.strings[node.name as usize].as_str());
            current = node.parent;
        }
        names.reverse();
        let local = names.join(".");
        match self.file_namespace(self.file_of(id)) {
            Some(namespace) if self.namespace_ancestor(id).is_none() => {
                format!("{namespace}.{local}")
            }
            _ => local,
        }
    }

    /// The namespace a C# declaration sits in (`""`: the global one).
    fn namespace_of(&self, id: u32) -> String {
        let mut names = Vec::new();
        let mut current = self.nodes[id as usize].parent;
        while current != NONE && self.nodes[current as usize].kind == NodeKind::Symbol {
            if self.symbol_kind(current) == "namespace" {
                names.push(self.strings[self.nodes[current as usize].name as usize].as_str());
            }
            current = self.nodes[current as usize].parent;
        }
        if names.is_empty() {
            return self.file_namespace(self.file_of(id)).unwrap_or_default();
        }
        names.reverse();
        names.join(".")
    }

    fn namespace_ancestor(&self, id: u32) -> Option<u32> {
        self.chain(Some(id))
            .find(|ancestor| self.symbol_kind(*ancestor) == "namespace")
    }

    /// A file's `namespace A.B;`: a top-level namespace with no members of
    /// its own (a block namespace holds its declarations).
    fn file_namespace(&self, file: u32) -> Option<String> {
        let memo = &self.bases.namespaces.0;
        if let Some(known) = memo.borrow().get(&file) {
            return known.clone();
        }
        let found = self
            .scope
            .path(file)
            .filter(|path| path.ends_with(ClassLang::CSharp.extension()))
            .and_then(|path| self.resolver.indexes.get(path))
            .and_then(|index| {
                let parents = index
                    .decl_nodes
                    .values()
                    .map(|id| self.nodes[*id as usize].parent)
                    .collect::<HashSet<_>>();
                let mut namespaces = index.decl_nodes.values().filter(|id| {
                    self.nodes[**id as usize].parent == NONE
                        && self.symbol_kind(**id) == "namespace"
                        && !parents.contains(*id)
                });
                let only = *namespaces.next()?;
                namespaces
                    .next()
                    .is_none()
                    .then(|| self.strings[self.nodes[only as usize].name as usize].clone())
            });
        memo.borrow_mut().insert(file, found.clone());
        found
    }
}
