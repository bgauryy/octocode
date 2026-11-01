//! Rust methods found through the receiver's type when that type's `impl`
//! blocks are spread over its crate: rust-analyzer collects every inherent
//! and trait `impl T` of the crate into `T`'s method set, so `self.m()` in one
//! `impl T` reaches the `m` another module's `impl T` declares.
use super::linker::{Linked, Linker, Site};
use super::*;

impl Linker<'_> {
    /// `self.m()` / `Self::m()` inside an `impl T` block that does not itself
    /// declare `m` (the caller's file did not settle it either): `T`'s method
    /// from another `impl T` block of the caller's crate.
    pub(super) fn rust_self_impl(&self, site: &Site) -> Option<Linked> {
        if !site.receiver()
            || matches!(site.qualifier, "super" | "super()")
            || !self.is_rust(site.caller)
        {
            return None;
        }
        let owner = self.nodes.get(site.caller as usize)?.parent;
        if owner == NONE || self.kind_of(owner) != "impl" {
            return None;
        }
        let type_name = self.strings[self.nodes[owner as usize].name as usize].as_str();
        let caller_path = self.scope.path(self.file_of(site.caller))?;
        let crate_dir = self.scope.rust_crate_dir(caller_path);
        self.rust_crate_member(site, type_name, crate_dir, "receiver-type")
    }

    /// The method `site.name` of the type named `type_name` among the `impl`
    /// blocks of the crate at `crate_dir`. An inherent method shadows a trait
    /// method; several remaining blocks are settled by scope or not at all.
    /// The type is matched by name, so the link is high only when the crate
    /// declares exactly one type of that name.
    pub(super) fn rust_crate_member(
        &self,
        site: &Site,
        type_name: &str,
        crate_dir: &str,
        via: &'static str,
    ) -> Option<Linked> {
        let in_crate = |id: u32| {
            self.scope
                .path(self.file_of(id))
                .is_some_and(|path| LinkScope::in_crate(path, crate_dir))
        };
        let members = self
            .members
            .get(&(type_name, site.name))
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| {
                let owner = self.nodes[*id as usize].parent;
                ((site.accept)(*id) || (self.recovered)(*id))
                    && owner != NONE
                    && self.kind_of(owner) == "impl"
                    && in_crate(*id)
            })
            .collect::<Vec<_>>();
        if members.is_empty() {
            return self.rust_trait_member(site, type_name);
        }
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
        let types = self
            .named(site.caller, type_name)
            .filter(|id| (self.is_type)(*id) && in_crate(*id))
            .count();
        let strong = if types == 1 && representatives.len() == 1 {
            Confidence::High
        } else {
            Confidence::Medium
        };
        let confidence = confidence_for(&targets, strong);
        Some((targets, via, confidence))
    }

    /// The caller a call in `owner` counts for: a `static` or `const` item
    /// inside a function body is part of that function, as rust-analyzer's
    /// call hierarchy counts its initializer's calls; any other owner is
    /// itself.
    pub(super) fn rust_item_caller(&self, owner: u32) -> u32 {
        let parent = self
            .nodes
            .get(owner as usize)
            .map_or(NONE, |node| node.parent);
        let nested_item = parent != NONE
            && self.is_rust(owner)
            && self.kind_of(owner) == "constant"
            && matches!(self.kind_of(parent), "function" | "method");
        if nested_item { parent } else { owner }
    }

    /// The declaration kind of symbol `id` (`impl`, `module`, `method`, …).
    pub(super) fn kind_of(&self, id: u32) -> &str {
        self.strings[self.nodes[id as usize].detail as usize].as_str()
    }
}
