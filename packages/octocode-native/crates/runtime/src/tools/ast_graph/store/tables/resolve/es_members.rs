//! Member calls through an ES import binding, bound the way TypeScript
//! binds them: `ns.f()` through `import * as ns` is the export `f` of that
//! module (following re-exports), and `X.m()` through `import { X }` or
//! `import X` is the member `m` of the declaration `X` names (a class's
//! static, an object literal's method, a namespace's function). A path
//! segment that is no name (`X.get().m`, `X[i].m`) has no static meaning.
//! No other declaration of the name is a candidate.
use super::linker::{Linked, Linker, Site};
use super::*;
use octocode_engine::graph::COMMONJS_WHOLE_EXPORT;

impl Linker<'_> {
    pub(super) fn import_member(&self, site: &Site, sources: &[(&str, &str)]) -> Option<Linked> {
        let path = site.qualifier.split('.').skip(1).collect::<Vec<_>>();
        if path
            .iter()
            .any(|segment| segment.is_empty() || segment.contains(['(', '[', '#']))
        {
            return None;
        }
        let mut uncertain = false;
        let mut modules = Vec::new();
        let mut owners = Vec::new();
        for (target, exported) in sources {
            if *exported == "*" {
                modules.push(*target);
                continue;
            }
            let mut resolved = self.resolver.resolved(target, exported);
            if *exported == "default" && resolved.targets.is_empty() {
                resolved = self.resolver.resolved(target, COMMONJS_WHOLE_EXPORT);
            }
            uncertain |= resolved.uncertain;
            owners.extend(resolved.targets);
        }
        let via = if owners.is_empty() {
            "namespace"
        } else {
            "import"
        };
        for segment in path {
            let mut next = Vec::new();
            for module in modules.drain(..) {
                let resolved = self.resolver.resolved(module, segment);
                uncertain |= resolved.uncertain;
                next.extend(resolved.targets);
            }
            for owner in owners {
                next.extend(self.children(owner, segment));
            }
            owners = next;
        }
        let mut targets = BTreeSet::new();
        let mut weakest = if uncertain {
            Confidence::Medium
        } else {
            Confidence::High
        };
        for module in modules {
            let resolved = self.resolver.resolved(module, site.name);
            if resolved.uncertain {
                weakest = weakest.max(Confidence::Medium);
            }
            targets.extend(site.accepted(resolved.targets));
        }
        for owner in owners {
            if let Some((members, _, strength)) =
                self.bound_member(site, owner, false, Confidence::High, via)
            {
                targets.extend(members);
                weakest = weakest.max(strength);
            }
        }
        let targets = targets.into_iter().collect::<Vec<_>>();
        let confidence = confidence_for(&targets, weakest);
        (!targets.is_empty()).then_some((targets, via, confidence))
    }

    /// Declarations directly inside `owner` named `name`.
    fn children(&self, owner: u32, name: &str) -> Vec<u32> {
        self.scope
            .path(self.file_of(owner))
            .and_then(|file| self.resolver.indexes.get(file))
            .and_then(|index| index.by_name.get(name))
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| self.nodes[*id as usize].parent == owner)
            .collect()
    }
}
