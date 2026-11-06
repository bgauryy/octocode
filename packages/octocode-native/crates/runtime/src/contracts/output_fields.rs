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
            let key = crate::json_pointer::unescape(segment);
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

/// Keywords that bound a string's value: such a value is meaningful only
/// whole, so a page never cuts it into windows.
const VALUE_BOUNDS: [&str; 6] = [
    "enum",
    "const",
    "format",
    "pattern",
    "minLength",
    "maxLength",
];

/// Whether the string at `pointer` (in a result row of `tool`, `/data/…`) is
/// free text a page may cut into windows: the tool's output schema, the one
/// the output validator checks, reaches it and bounds its value nowhere.
/// A string the schema does not reach stays whole.
pub(crate) fn free_text_at(tool: ToolId, pointer: &str) -> bool {
    use super::schema_facts::{child_schemas, item_schemas, property_schemas};
    let Ok(contract) = super::tool_contract(tool) else {
        return false;
    };
    let schema = &contract["outputSchema"];
    let defs = &schema["$defs"];
    let mut at = property_schemas(schema, defs, "results")
        .into_iter()
        .flat_map(|results| item_schemas(results, defs))
        .collect::<Vec<_>>();
    for segment in pointer.strip_prefix('/').unwrap_or(pointer).split('/') {
        if pointer.is_empty() {
            break;
        }
        let key = crate::json_pointer::unescape(segment);
        at = at
            .into_iter()
            .flat_map(|schema| {
                let children = child_schemas(schema, defs, &key);
                if children.is_empty() && segment.parse::<usize>().is_ok() {
                    item_schemas(schema, defs)
                } else {
                    children
                }
            })
            .collect();
    }
    !at.is_empty()
        && at
            .iter()
            .all(|schema| !bounds_value(schema, defs, &mut Vec::new()))
}

/// Whether `schema`, through references and composition branches, bounds a
/// value with any of [`VALUE_BOUNDS`].
fn bounds_value<'a>(
    schema: &'a serde_json::Value,
    defs: &'a serde_json::Value,
    seen: &mut Vec<*const serde_json::Value>,
) -> bool {
    if seen.contains(&std::ptr::from_ref(schema)) {
        return false;
    }
    seen.push(std::ptr::from_ref(schema));
    if VALUE_BOUNDS
        .iter()
        .any(|keyword| super::schema_facts::schema_keyword(schema, defs, keyword).is_some())
    {
        return true;
    }
    let resolved = super::schema_facts::resolve_ref(schema, defs);
    ["anyOf", "oneOf", "allOf"].iter().any(|keyword| {
        resolved[*keyword]
            .as_array()
            .into_iter()
            .flatten()
            .any(|branch| bounds_value(branch, defs, seen))
    })
}

pub(crate) struct PageContextFields {
    pub(crate) required: BTreeSet<String>,
    pub(crate) optional: BTreeSet<String>,
    pub(crate) collection_routes: CollectionRoutes,
    /// Declared collections at a fixed place in a row (no `*` on the way).
    pub(crate) fixed_paths: Vec<String>,
    /// Fixed collections and holders the schema lets a part omit.
    pub(crate) optional_paths: BTreeSet<String>,
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
        let pointers = |field: &str| {
            contract
                .get(field)
                .expect("core declares collection routes for every tool")
                .as_array()
                .expect("core collection paths are an array")
                .iter()
                .map(|path| {
                    path.as_str()
                        .expect("core collection path is a string")
                        .to_owned()
                })
                .collect::<Vec<_>>()
        };
        let paths = pointers("collectionPaths");
        let collection_routes = {
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
            fixed_paths: paths
                .iter()
                .filter(|path| !path.split('/').any(|segment| segment == "*"))
                .cloned()
                .collect(),
            optional_paths: pointers("optionalCollectionPaths").into_iter().collect(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bounded value (the clasify answer `type` enum) is never free text;
    /// file content is; a path the schema does not reach stays whole.
    #[test]
    fn free_text_follows_the_output_schema() {
        assert!(!free_text_at(ToolId::Clasify, "/data/answers/q0/type"));
        assert!(!free_text_at(ToolId::Clasify, "/data/questions/q0/type"));
        assert!(free_text_at(ToolId::GhGetFileContent, "/data/content"));
        assert!(!free_text_at(ToolId::GhGetFileContent, "/data/undeclared"));
    }

    #[test]
    fn every_tool_declares_disjoint_page_context_fields() {
        for tool in ToolId::ALL {
            let context = page_context_fields(tool);
            assert!(context.required.is_disjoint(&context.optional), "{tool:?}");
        }
    }

    #[test]
    fn optional_collection_slots_sit_at_fixed_places() {
        for tool in ToolId::ALL {
            let context = page_context_fields(tool);
            assert!(
                context
                    .optional_paths
                    .iter()
                    .all(|path| !path.contains("/*")),
                "{tool:?}"
            );
        }
        for tool in [
            ToolId::LocalSearch,
            ToolId::StructureSearch,
            ToolId::AstSearch,
        ] {
            let context = page_context_fields(tool);
            assert!(context.optional_paths.contains("/data/skippedPaths"));
            assert!(
                !context
                    .optional_paths
                    .contains("/data/skippedPaths/entries")
            );
            assert!(!context.optional_paths.contains("/data/files"));
        }
    }
}
