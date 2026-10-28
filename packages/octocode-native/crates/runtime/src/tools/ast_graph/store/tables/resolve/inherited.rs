use super::linker::Linked;
use super::*;

#[derive(Default)]
pub(crate) struct BaseIndex {
    parents: HashMap<u32, Vec<(u32, Confidence)>>,
    unresolved: HashSet<u32>,
}

pub(super) enum TypedMember {
    Unbound,
    Missing,
    Linked(Linked),
}

pub(crate) fn base_index(edges: &[PendingEdge], unresolved: HashSet<u32>) -> BaseIndex {
    let mut bases = BaseIndex {
        unresolved,
        ..BaseIndex::default()
    };
    for (from, to, kind, confidence, detail, _) in edges {
        if *kind == EdgeKind::Inherits && detail.starts_with("extends:") {
            bases
                .parents
                .entry(*from)
                .or_default()
                .push((*to, *confidence));
        }
    }
    bases
}

impl Linker<'_> {
    pub(super) fn receiver_owner(&self, site: &Site) -> Option<u32> {
        let owner = self.nodes.get(site.caller as usize)?.parent;
        (owner != NONE && (self.is_type)(owner)).then_some(owner)
    }

    /// Resolve each inheritance branch until the first declaration of the
    /// name. An override stops that branch; a diamond deduplicates identity,
    /// and revisiting a type cannot make a cycle loop forever.
    pub(super) fn bound_member(
        &self,
        site: &Site,
        owner: u32,
        skip_owner: bool,
        strong: Confidence,
        direct_via: &'static str,
    ) -> Option<Linked> {
        let mut pending = if skip_owner {
            self.bases
                .parents
                .get(&owner)?
                .iter()
                .map(|(id, confidence)| (*id, strong.max(*confidence), true))
                .collect()
        } else {
            vec![(owner, strong, false)]
        };
        let mut seen = HashMap::<u32, Confidence>::new();
        let mut found = BTreeMap::<u32, Confidence>::new();
        let mut inherited = false;
        let mut uncertain = skip_owner && self.bases.unresolved.contains(&owner);
        while let Some((current, confidence, through_base)) = pending.pop() {
            if seen.get(&current).is_some_and(|old| *old <= confidence) {
                continue;
            }
            seen.insert(current, confidence);
            let declarations = self
                .scope
                .path(self.nodes[current as usize].file)
                .and_then(|file| self.resolver.indexes.get(file))
                .and_then(|index| index.by_name.get(site.name))
                .into_iter()
                .flatten()
                .filter(|id| self.nodes[**id as usize].parent == current)
                .copied()
                .collect::<Vec<_>>();
            if !declarations.is_empty() {
                for id in declarations.into_iter().filter(|id| (site.accept)(*id)) {
                    found
                        .entry(id)
                        .and_modify(|old| *old = (*old).min(confidence))
                        .or_insert(confidence);
                    inherited |= through_base;
                }
                continue;
            }
            uncertain |= self.bases.unresolved.contains(&current);
            for (base, edge_confidence) in self.bases.parents.get(&current).into_iter().flatten() {
                pending.push((*base, confidence.max(*edge_confidence), true));
            }
        }
        let mut confidence = found.values().copied().max()?;
        if uncertain {
            confidence = confidence.max(Confidence::Medium);
        }
        let targets = found.into_keys().collect::<Vec<_>>();
        let confidence = confidence_for(&targets, confidence);
        Some((
            targets,
            if inherited { "inherited" } else { direct_via },
            confidence,
        ))
    }

    /// Bind the receiver's type through its own import/local scope before
    /// searching bases. Rust impl members use the existing type-name index.
    pub(super) fn typed_member(&self, site: &Site, name: &str, via: &'static str) -> TypedMember {
        let type_site = Site {
            bindings: site.bindings,
            index: site.index,
            caller: site.caller,
            qualifier: "",
            name,
            accept: self.is_type,
            receiver_type: None,
        };
        let Some((owners, _, confidence)) = self.link(&type_site) else {
            return TypedMember::Unbound;
        };
        if owners.iter().any(|id| {
            self.scope
                .path(self.nodes[*id as usize].file)
                .is_some_and(|p| p.ends_with(".rs"))
        }) {
            return TypedMember::Unbound;
        }
        let mut targets = BTreeSet::new();
        let mut weakest = confidence;
        let mut inherited = false;
        for owner in owners {
            if let Some((members, resolved_via, strength)) =
                self.bound_member(site, owner, false, confidence, via)
            {
                targets.extend(members);
                weakest = weakest.max(strength);
                inherited |= resolved_via == "inherited";
            }
        }
        let targets = targets.into_iter().collect::<Vec<_>>();
        let weakest = confidence_for(&targets, weakest);
        if targets.is_empty() {
            return TypedMember::Missing;
        }
        TypedMember::Linked((targets, if inherited { "inherited" } else { via }, weakest))
    }
}
