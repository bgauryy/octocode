//! C++ and CUDA qualified calls, by C++ qualified name lookup: `ns::f()`,
//! `A::B::f()` and `::ns::f()` name a function whose enclosing namespaces
//! and classes end with the written path (or, after a leading `::`, are
//! exactly it). A namespace reopened in several files is one scope, so the
//! match compares names, never the namespace declarations themselves.
use super::linker::{Linked, Linker, Site};
use super::*;

impl Linker<'_> {
    /// A C-family call through a `::` path: a qualifier that is no value
    /// (see [`Site::value_qualifier`]) and no `this`.
    pub(super) fn c_family_path(&self, site: &Site) -> bool {
        !site.qualifier.is_empty()
            && !site.value_qualifier
            && !site.receiver()
            && self.scope.families.get(self.file_of(site.caller) as usize) == Some(&"system")
    }

    /// The declarations a [`Self::c_family_path`] call names; `None` when
    /// none, or several scopes the caller's scope cannot choose, match.
    pub(super) fn c_family_qualified(&self, site: &Site) -> Option<Linked> {
        let absolute = site.qualifier.starts_with("::");
        let written = site
            .qualifier
            .split("::")
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        let matching = self
            .named(site.caller, site.name)
            .filter(|id| (site.accept)(*id) || (self.recovered)(*id))
            .filter_map(|id| {
                let scope = self.scope_of(id)?;
                let fits = if absolute {
                    scope == written
                } else {
                    scope.ends_with(&written)
                };
                fits.then_some((scope, id))
            })
            .collect::<Vec<_>>();
        // A C++ caller sees what its translation unit declares: its own
        // file, its directory, the headers it includes. A match only in an
        // unrelated file (a vendored copy, another program) is a name match.
        let caller_file = self.file_of(site.caller);
        let near = matching
            .iter()
            .filter(|(_, id)| self.file_of(*id) == caller_file || self.near(site, *id))
            .cloned()
            .collect::<Vec<_>>();
        let far = near.is_empty();
        let matching = if far { matching } else { near };
        let (first, _) = matching.first()?;
        let pool = matching.iter().map(|(_, id)| *id).collect::<Vec<_>>();
        let (pool, via, strong) = if matching.iter().all(|(scope, _)| scope == first) {
            // Overloads of one scope all stay candidates; a scope the call
            // spells completely is its exact name.
            let strong = if first.len() == written.len() {
                Confidence::High
            } else {
                Confidence::Medium
            };
            (pool, "qualified", strong)
        } else {
            let (id, via) = self.pick(site, &pool)?;
            (vec![id], via, Confidence::Medium)
        };
        // A recovered declaration in the choice may be the one meant.
        let targets = site.accepted(pool.iter().copied());
        if targets.is_empty() || targets.len() < pool.len() {
            return None;
        }
        let confidence = if far {
            Confidence::Low
        } else {
            confidence_for(&targets, strong)
        };
        Some((targets, via, confidence))
    }

    /// Names of the namespaces and classes enclosing `id`, outermost first.
    fn scope_of(&self, id: u32) -> Option<Vec<&str>> {
        let mut scope = Vec::new();
        let mut parent = self.nodes.get(id as usize)?.parent;
        while parent != NONE {
            let node = self.nodes.get(parent as usize)?;
            scope.push(self.strings.get(node.name as usize)?.as_str());
            parent = node.parent;
            if scope.len() > 64 {
                return None;
            }
        }
        scope.reverse();
        Some(scope)
    }
}
