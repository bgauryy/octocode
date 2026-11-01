//! Go method sets, as gopls resolves `x.M()`: the methods declared on
//! `x`'s named type anywhere in its package (value and pointer receivers
//! alike), else the shallowest methods promoted from embedded fields; an
//! interface's own and embedded method specs. Two promotions at one depth
//! are ambiguous, and an unresolved embedded type may hide the method.
use super::inherited::TypedMember;
use super::linker::{Linker, Site};
use super::*;

impl Linker<'_> {
    pub(super) fn is_go(&self, caller: u32) -> bool {
        let file = self.file_of(caller);
        self.scope.families.get(file as usize).copied() == Some("go")
    }

    /// `x.name()` with `x` of the type spelled `type_head` (or, for a typed
    /// receiver, its written `pkg.T`, bound through the file's import).
    pub(super) fn go_typed_member(
        &self,
        site: &Site,
        type_head: &str,
        via: &'static str,
    ) -> TypedMember {
        let written = site
            .receiver_type
            .filter(|written| written.rsplit('.').next() == Some(type_head))
            .unwrap_or(type_head);
        let (qualifier, name) = written.rsplit_once('.').unwrap_or(("", written));
        let type_site = Site {
            bindings: site.bindings,
            index: site.index,
            caller: site.caller,
            qualifier,
            name,
            accept: self.is_type,
            receiver_type: None,
            term_binding: false,
            self_template: None,
            value_qualifier: false,
        };
        let Some((owners, _, confidence)) = self.link(&type_site) else {
            return TypedMember::Unbound;
        };
        let mut targets = BTreeSet::new();
        let mut weakest = confidence;
        let mut promoted = false;
        for owner in owners {
            if let Some((methods, deeper, uncertain)) = self.method_set(site, owner) {
                targets.extend(methods);
                promoted |= deeper;
                if uncertain {
                    weakest = weakest.max(Confidence::Medium);
                }
            }
        }
        let targets = targets.into_iter().collect::<Vec<_>>();
        if targets.is_empty() {
            return TypedMember::Missing;
        }
        let confidence = confidence_for(&targets, weakest);
        let via = if promoted { "inherited" } else { via };
        TypedMember::Linked((targets, via, confidence))
    }

    /// The shallowest methods named `site.name` of `owner`'s method set,
    /// whether they were promoted, and whether an unresolved embedded type
    /// at or above that depth could also supply the name.
    fn method_set(&self, site: &Site, owner: u32) -> Option<(Vec<u32>, bool, bool)> {
        let mut level = vec![owner];
        let mut seen = HashSet::new();
        let mut uncertain = false;
        for depth in 0..self.nodes.len() {
            let mut found = Vec::new();
            let mut next = Vec::new();
            for ty in level.into_iter().filter(|ty| seen.insert(*ty)) {
                found.extend(self.declared_methods(site, ty));
                uncertain |= self.bases.unresolved.contains(&ty);
                let bases = self.bases.parents.get(&ty).into_iter().flatten();
                next.extend(bases.map(|(base, _)| *base));
            }
            if !found.is_empty() {
                return Some((found, depth > 0, uncertain));
            }
            if next.is_empty() {
                break;
            }
            level = next;
        }
        None
    }

    /// Methods named `site.name` declared on the type node `ty`: nested in
    /// it (an interface's method specs) or on its name in its package.
    fn declared_methods(&self, site: &Site, ty: u32) -> Vec<u32> {
        let node = &self.nodes[ty as usize];
        let type_name = self.strings[node.name as usize].as_str();
        let package = self
            .scope
            .path(node.file)
            .map(crate::tools::ast_graph::paths::parent);
        self.members
            .get(&(type_name, site.name))
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| {
                let method = &self.nodes[*id as usize];
                method.parent == ty
                    || (method.parent == NONE
                        && self
                            .scope
                            .path(method.file)
                            .map(crate::tools::ast_graph::paths::parent)
                            == package)
            })
            .filter(|id| (site.accept)(*id))
            .collect()
    }
}
