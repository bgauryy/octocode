//! Scope choices for graph-wide candidates: a type's members (overloads
//! included) and the caller's package or imports deciding among them.
use super::linker::{Linked, Linker, Site};
use super::*;

impl Linker<'_> {
    /// `Type::method` / `Type.method`, or `x.method()` with `x: Type`: a
    /// member of a container named `Type`.
    pub(super) fn member(&self, site: &Site, type_head: &str, via: &'static str) -> Option<Linked> {
        match self.typed_member(site, type_head, via) {
            super::inherited::TypedMember::Linked(resolved) => return Some(resolved),
            super::inherited::TypedMember::Missing => return None,
            super::inherited::TypedMember::Unbound => {}
        }
        // Recovered members keep their type in the choice (see `global`);
        // another language's same-named type is never a candidate.
        let members = self
            .members
            .get(&(type_head, site.name))
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| {
                ((site.accept)(*id) || (self.recovered)(*id)) && self.same_family(site.caller, *id)
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            return self.rust_trait_member(site, type_head);
        }
        // Overloads share one container: choose the type, then link every
        // overload it declares under the name.
        let mut owners = members
            .iter()
            .map(|id| (self.nodes[*id as usize].parent, *id))
            .collect::<Vec<_>>();
        owners.sort_unstable();
        owners.dedup_by_key(|(owner, _)| *owner);
        let representatives = owners.iter().map(|(_, id)| *id).collect::<Vec<_>>();
        let representatives = self.rust_inherent_first(site.caller, representatives);
        let id = match representatives.as_slice() {
            [] => return None,
            [only] => *only,
            many => self.pick(site, many)?.0,
        };
        let owner = self.nodes[id as usize].parent;
        let targets = site.accepted(
            members
                .into_iter()
                .filter(|member| self.nodes[*member as usize].parent == owner),
        );
        if targets.is_empty() {
            return None;
        }
        let confidence = if self.near(site, id) {
            Confidence::High
        } else {
            Confidence::Medium
        };
        let confidence = confidence_for(&targets, confidence);
        Some((targets, via, confidence))
    }

    /// Scope preference: the caller's package (directory), then files the
    /// caller imports; a choice is made only when it is unique.
    pub(super) fn pick(&self, site: &Site, pool: &[u32]) -> Option<(u32, &'static str)> {
        let caller_dir = self.dir(self.file_of(site.caller));
        let same_package = pool
            .iter()
            .copied()
            .filter(|id| self.dir(self.file_of(*id)) == caller_dir)
            .collect::<Vec<_>>();
        if let [only] = same_package.as_slice() {
            return Some((*only, "same-package"));
        }
        let imported = pool
            .iter()
            .copied()
            .filter(|id| self.imported_by(site, *id))
            .collect::<Vec<_>>();
        if let [only] = imported.as_slice() {
            return Some((*only, "import-scope"));
        }
        None
    }

    /// The target sits in the caller's package or in a file it imports.
    pub(super) fn near(&self, site: &Site, id: u32) -> bool {
        self.dir(self.file_of(id)) == self.dir(self.file_of(site.caller))
            || self.imported_by(site, id)
    }

    fn imported_by(&self, site: &Site, id: u32) -> bool {
        self.scope
            .path(self.file_of(id))
            .is_some_and(|path| site.bindings.imported_files.contains(path))
    }

    pub(super) fn file_of(&self, id: u32) -> u32 {
        self.nodes.get(id as usize).map_or(NONE, |n| n.file)
    }

    fn dir(&self, file: u32) -> &str {
        self.scope
            .path(file)
            .map_or("", crate::tools::ast_graph::graph::parent)
    }
}
