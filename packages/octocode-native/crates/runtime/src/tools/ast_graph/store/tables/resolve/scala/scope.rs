//! Scope lookups for Scala linking: declaration chains, members by name,
//! and the kind of a declaration.
use super::super::linker::{Linker, Site};
use super::super::*;

impl Linker<'_> {
    /// An object path (`Outer`, `Outer.Inner`) declared in the caller's file.
    pub(super) fn file_object_path(&self, site: &Site, path: &[&str]) -> Option<u32> {
        let (first, rest) = path.split_first()?;
        let top = self.declared_by(None, site.caller, first);
        let mut module = unique(
            &top.into_iter()
                .filter(|id| self.is_module(*id))
                .collect::<Vec<_>>(),
        )?;
        for name in rest {
            let nested = self.declared_by(Some(module), module, name);
            module = unique(
                &nested
                    .into_iter()
                    .filter(|id| self.is_module(*id))
                    .collect::<Vec<_>>(),
            )?;
        }
        Some(module)
    }

    /// `start` and its enclosing declarations, innermost first.
    pub(in super::super) fn chain(&self, start: Option<u32>) -> impl Iterator<Item = u32> + '_ {
        let symbol = |id: &u32| {
            self.nodes
                .get(*id as usize)
                .is_some_and(|node| node.kind == NodeKind::Symbol)
        };
        std::iter::successors(start.filter(symbol), move |id| {
            Some(self.nodes[*id as usize].parent).filter(symbol)
        })
    }

    /// The caller's scopes, innermost first, then the file's top level.
    pub(super) fn scopes(&self, caller: u32) -> impl Iterator<Item = Option<u32>> + '_ {
        self.chain(Some(caller)).map(Some).chain([None])
    }

    /// Members of `scope` (top-level declarations for `None`) named `name`,
    /// in the file of `anchor`.
    pub(in super::super) fn declared_by(
        &self,
        scope: Option<u32>,
        anchor: u32,
        name: &str,
    ) -> Vec<u32> {
        let parent = scope.unwrap_or(NONE);
        self.scope
            .path(self.file_of(anchor))
            .and_then(|file| self.resolver.indexes.get(file))
            .and_then(|index| index.by_name.get(name))
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| self.nodes[*id as usize].parent == parent)
            .collect()
    }

    pub(in super::super) fn symbol_kind(&self, id: u32) -> &str {
        let detail = self.nodes[id as usize].detail;
        self.strings.get(detail as usize).map_or("", String::as_str)
    }

    pub(super) fn is_module(&self, id: u32) -> bool {
        self.symbol_kind(id) == "module"
    }

    /// A class, trait or object, or a declaration whose value is an
    /// anonymous class (it carries bases).
    pub(super) fn is_template(&self, id: u32) -> bool {
        matches!(self.symbol_kind(id), "class" | "trait" | "module" | "enum")
            || self.bases.scala.contains_key(&id)
    }

    pub(super) fn scala_node(&self, id: u32) -> bool {
        self.scope.path(self.file_of(id)).is_some_and(|path| {
            [".scala", ".sc", ".sbt"]
                .iter()
                .any(|ext| path.ends_with(ext))
        })
    }

    pub(in super::super) fn dir_of(&self, file: u32) -> &str {
        self.scope
            .path(file)
            .map_or("", crate::tools::ast_graph::paths::parent)
    }
}

pub(super) fn upper(segment: &str) -> bool {
    segment.starts_with(|c: char| c.is_ascii_uppercase())
}

pub(super) fn unique(ids: &[u32]) -> Option<u32> {
    match ids {
        [only] => Some(*only),
        _ => None,
    }
}
