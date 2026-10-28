//! Core-declared fields retained on output fragments.
use crate::id::ToolId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct CollectionRoute {
    pub(crate) children: BTreeMap<String, usize>,
    pub(crate) references: Vec<usize>,
    pub(crate) collection: bool,
}

/// A finite trie of declared collection paths with exact schema-reference edges.
#[derive(Default)]
pub(crate) struct CollectionRoutes {
    pub(crate) nodes: Vec<CollectionRoute>,
}

impl CollectionRoutes {
    pub(crate) fn new(paths: &[String], references: &[(String, String)]) -> Self {
        let mut routes = Self::default();
        for path in paths {
            let node = routes.insert(path);
            routes.nodes[node].collection = true;
        }
        for (path, target) in references {
            let from = routes.insert(path);
            let to = routes.insert(target);
            if !routes.nodes[from].references.contains(&to) {
                routes.nodes[from].references.push(to);
            }
        }
        routes
    }

    fn insert(&mut self, path: &str) -> usize {
        if self.nodes.is_empty() {
            self.nodes.push(CollectionRoute::default());
        }
        let mut at = 0;
        for segment in path.strip_prefix('/').unwrap_or(path).split('/') {
            if path.is_empty() {
                break;
            }
            let key = segment.replace("~1", "/").replace("~0", "~");
            at = match self.nodes[at].children.get(&key) {
                Some(next) => *next,
                None => {
                    let next = self.nodes.len();
                    self.nodes.push(CollectionRoute::default());
                    self.nodes[at].children.insert(key, next);
                    next
                }
            };
        }
        at
    }
}

pub(crate) struct PageContextFields {
    pub(crate) required: BTreeSet<String>,
    pub(crate) optional: BTreeSet<String>,
    pub(crate) collection_routes: CollectionRoutes,
}

#[expect(
    clippy::expect_used,
    reason = "missing embedded page context is a contract defect caught for every tool by contract tests"
)]
pub(crate) fn page_context_fields(tool: ToolId) -> &'static PageContextFields {
    static FIELDS: [std::sync::OnceLock<PageContextFields>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    FIELDS[tool.index()].get_or_init(|| {
        let contract = super::tool_contract(tool).expect("embedded contract contains every tool");
        let context = &contract["pageContextFields"];
        let names = |kind: &str| {
            context[kind]
                .as_array()
                .expect("core declares each tool's page context")
                .iter()
                .map(|field| field.as_str().expect("page context field name").to_owned())
                .collect()
        };
        let collection_routes = {
            let value = contract
                .get("collectionPaths")
                .expect("core declares collection routes for every tool");
            let paths = value
                .as_array()
                .expect("core collection paths are an array")
                .iter()
                .map(|path| {
                    path.as_str()
                        .expect("core collection path is a string")
                        .to_owned()
                })
                .collect::<Vec<_>>();
            let references = contract
                .get("collectionRefs")
                .map(|value| {
                    value
                        .as_array()
                        .expect("core collection references are an array")
                })
                .into_iter()
                .flatten()
                .map(|reference| {
                    let path = reference["path"]
                        .as_str()
                        .expect("collection reference path");
                    let target = reference["target"]
                        .as_str()
                        .expect("collection reference target");
                    (path.to_owned(), target.to_owned())
                })
                .collect::<Vec<_>>();
            CollectionRoutes::new(&paths, &references)
        };
        PageContextFields {
            required: names("required"),
            optional: names("optional"),
            collection_routes,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_declares_disjoint_page_context_fields() {
        for tool in ToolId::ALL {
            let context = page_context_fields(tool);
            assert!(context.required.is_disjoint(&context.optional), "{tool:?}");
        }
    }
}
