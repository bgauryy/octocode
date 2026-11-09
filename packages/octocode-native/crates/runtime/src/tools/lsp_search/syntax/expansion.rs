//! The syntax rung's call and type hierarchies: walked by name from the anchor
//! declaration, `depth` levels, each node expanded once, one level of a
//! node's relations read from the tree-sitter facts of the files that spell it.
use super::hierarchy::{self, Found};
use super::*;
use crate::tools::lsp_search::walk::{
    Expansion, HierarchyEdge, call_items_payload, public_edge, scope_callers,
};
use std::collections::HashSet;

impl Request<'_> {
    /// A call or type hierarchy walked by name, `depth` levels from the
    /// anchor declaration, each node expanded once.
    pub(super) async fn hierarchy(&self, site: &Site, op: Op, name: &str) -> Result<Value, Exit> {
        let expansion = match op {
            Op::Callers => Expansion::IncomingCalls,
            Op::Callees => Expansion::OutgoingCalls,
            Op::Supertypes => Expansion::Supertypes,
            _ => Expansion::Subtypes,
        };
        let root = hierarchy::anchor_declaration(&site.file, name, site.line);
        let mut frontier = root.into_iter().collect::<Vec<Found>>();
        let mut seen = frontier.iter().map(Found::key).collect::<HashSet<_>>();
        let mut edges = Vec::new();
        for level in 1..=lsp_search_query::depth(self.query).unwrap_or(1).max(1) {
            let mut next = Vec::new();
            for parent in std::mem::take(&mut frontier) {
                for (found, sites) in self.expand(site, expansion, &parent).await? {
                    if seen.insert(found.key()) {
                        next.push(found.clone());
                    }
                    edges.push(HierarchyEdge {
                        node: found.node,
                        parent: Some(parent.node.clone()),
                        level,
                        sites,
                    });
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        let mut items = edges
            .iter()
            .map(|edge| public_edge(expansion, edge, None))
            .collect::<Vec<_>>();
        Ok(if expansion.is_call() {
            let excluded = scope_callers(self.query, self.paths, &mut items);
            let mut row = call_items_payload(self.query, &items, None, self.budget, self.paths);
            crate::tools::lsp_search::test_scope::disclose(&mut row, excluded);
            row
        } else {
            items_payload(self.query, op.name(), json!(items), self.budget)
        })
    }

    /// One level of `parent`'s hierarchy: each related declaration and the
    /// call sites between them (none for a type relation).
    async fn expand(
        &self,
        site: &Site,
        expansion: Expansion,
        parent: &Found,
    ) -> Result<Vec<(Found, Vec<Value>)>, Exit> {
        let name = parent.name().to_owned();
        match expansion {
            Expansion::IncomingCalls => {
                let files = self.spelling(site, &name).await?;
                let names = BTreeSet::from([name]);
                let calls = self
                    .blocking(move |stopped| hierarchy::incoming(&files, &names, stopped))
                    .await?;
                Ok(calls
                    .into_iter()
                    .map(|(_, caller, sites)| (caller, sites))
                    .collect())
            }
            Expansion::OutgoingCalls => {
                let (path, id) = parent.key();
                let calls = self
                    .blocking(move |_| hierarchy::outgoing(&path, &id))
                    .await?;
                let mut out = Vec::new();
                for (callee, sites) in calls {
                    for found in self.declarations(site, &callee).await? {
                        out.push((found, sites.clone()));
                    }
                }
                Ok(out)
            }
            Expansion::Supertypes => {
                let (path, id) = parent.key();
                let bases = self.blocking(move |_| hierarchy::bases(&path, &id)).await?;
                let mut out = Vec::new();
                for base in bases {
                    for found in self.declarations(site, &base).await? {
                        out.push((found, Vec::new()));
                    }
                }
                Ok(out)
            }
            Expansion::Subtypes => {
                let files = self.spelling(site, &name).await?;
                let found = self
                    .blocking(move |stopped| {
                        hierarchy::declarations_at(
                            &heritage::scan_subtypes(&files, &name, stopped).0,
                        )
                    })
                    .await?;
                Ok(found.into_iter().map(|found| (found, Vec::new())).collect())
            }
        }
    }

    /// The declarations named `name` in the files of the scope that spell it.
    async fn declarations(&self, site: &Site, name: &str) -> Result<Vec<Found>, Exit> {
        let files = self.spelling(site, name).await?;
        let name = name.to_owned();
        self.blocking(move |stopped| hierarchy::named(&files, &name, stopped))
            .await
    }
}
