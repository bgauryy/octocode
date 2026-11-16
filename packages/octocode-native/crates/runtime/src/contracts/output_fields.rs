//! Core-declared fields retained on output fragments.
use crate::id::ToolId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct CollectionRoute {
    pub(crate) children: BTreeMap<String, usize>,
    pub(crate) references: Vec<usize>,
    pub(crate) collection: bool,
}

/// A finite trie of declared row paths (`*` for any element or record key)
/// with exact schema-reference edges.
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

    /// The nodes a concrete row pointer reaches: each segment by its key or
    /// `*`, and a reference edge wherever one leaves a reached node.
    fn reach(&self, pointer: &str) -> Vec<usize> {
        if self.nodes.is_empty() {
            return Vec::new();
        }
        let mut states = self.with_references(vec![0]);
        for segment in pointer.strip_prefix('/').unwrap_or(pointer).split('/') {
            if pointer.is_empty() || states.is_empty() {
                break;
            }
            let key = crate::json_pointer::unescape(segment);
            let next = states
                .iter()
                .flat_map(|state| {
                    let children = &self.nodes[*state].children;
                    [children.get(&key), children.get("*")]
                })
                .flatten()
                .copied()
                .collect();
            states = self.with_references(next);
        }
        states
    }

    /// `states` and every node their reference edges lead to.
    fn with_references(&self, mut states: Vec<usize>) -> Vec<usize> {
        let mut index = 0;
        while let Some(state) = states.get(index).copied() {
            for target in &self.nodes[state].references {
                if !states.contains(target) {
                    states.push(*target);
                }
            }
            index += 1;
        }
        states
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

/// Keywords that bound a list's size or shape: a list one of them
/// constrains may have no valid empty form, so a later part keeps it whole.
const LIST_BOUNDS: [&str; 6] = [
    "minItems",
    "prefixItems",
    "contains",
    "minContains",
    "enum",
    "const",
];

/// Keywords that bound a record's size or value: such a record may have no
/// valid empty form, so a later part keeps it whole.
const RECORD_BOUNDS: [&str; 3] = ["minProperties", "enum", "const"];

/// Whether the string at `pointer` (in a result row of `tool`, `/data/…`) is
/// free text a page may cut into windows: the tool's output schema, the one
/// the output validator checks, reaches it and bounds its value nowhere.
/// A string the schema does not reach stays whole.
pub(crate) fn free_text_at(tool: ToolId, pointer: &str) -> bool {
    unbounded_at(tool, pointer, &VALUE_BOUNDS)
}

/// Whether the output schema admits `value`'s empty form (`""`, `[]`, an
/// object without its undeclared members) at `pointer`, as
/// [`free_text_at`] decides for a string: the schema reaches the value and
/// bounds it nowhere. Any other value has no empty form.
pub(crate) fn emptiable_at(tool: ToolId, pointer: &str, value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(_) => free_text_at(tool, pointer),
        serde_json::Value::Array(_) => unbounded_at(tool, pointer, &LIST_BOUNDS),
        serde_json::Value::Object(_) => unbounded_at(tool, pointer, &RECORD_BOUNDS),
        _ => false,
    }
}

/// Whether `tool`'s output schema reaches row pointer `pointer` and, on
/// every schema reaching it, bounds the value with none of `bounds`.
fn unbounded_at(tool: ToolId, pointer: &str, bounds: &[&str]) -> bool {
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
            .all(|schema| !bounds_value(schema, defs, bounds, &mut Vec::new()))
}

/// Whether `schema`, through references and composition branches, bounds a
/// value with any of `bounds`; a minimum of zero bounds nothing.
fn bounds_value<'a>(
    schema: &'a serde_json::Value,
    defs: &'a serde_json::Value,
    bounds: &[&str],
    seen: &mut Vec<*const serde_json::Value>,
) -> bool {
    if seen.contains(&std::ptr::from_ref(schema)) {
        return false;
    }
    seen.push(std::ptr::from_ref(schema));
    if bounds.iter().any(|keyword| {
        super::schema_facts::schema_keyword(schema, defs, keyword)
            .is_some_and(|bound| bound.as_u64() != Some(0))
    }) {
        return true;
    }
    let resolved = super::schema_facts::resolve_ref(schema, defs);
    ["anyOf", "oneOf", "allOf"].iter().any(|keyword| {
        resolved[*keyword]
            .as_array()
            .into_iter()
            .flatten()
            .any(|branch| bounds_value(branch, defs, bounds, seen))
    })
}

/// A response-level receipt: a `data` member that describes the whole
/// answer, not its rows, so a page after the first of one answer need not
/// restate it. Every page keeps what it needs to be cited and continued
/// alone (locators, every `next` call, and its own `related` leads, which
/// name that page's rows); a receipt is stated on the first page. The output
/// schema requires none of them on any tool.
pub(crate) struct PageOneReceipt {
    /// The member's name on a result row's `data`.
    pub(crate) member: &'static str,
    /// Whether `value` is a receipt and not a disclosure a later page must
    /// still state.
    pub(crate) is_receipt: fn(&serde_json::Value) -> bool,
}

/// The receipt class, one declaration for every tool: a `coverage` block
/// that reports a complete search (the same fact on every page; a partial
/// one is a disclosure and rides every page).
pub(crate) const PAGE_ONE_RECEIPTS: [PageOneReceipt; 1] = [PageOneReceipt {
    member: "coverage",
    is_receipt: |coverage| coverage["state"] == "complete",
}];

/// What a later part of a split row keeps of a member, as core declares it
/// (`pageContextFields`). Ordered by precedence: a member two declared paths
/// reach (through a recursive reference) takes the greater.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PageContext {
    /// An optional identity, next or disclosure field: repeated whole when
    /// present.
    Optional,
    /// A required evidence field: delivered once, kept on a later part only
    /// so the part validates against the published output schema, which an
    /// MCP client checks `structuredContent` against
    /// ([`PageContextFields::keep_later`] empties it).
    RequiredEvidence,
    /// A required identity, next or disclosure field: repeated whole.
    Required,
}

/// Whether `map` is an executable call (`tool` beside `input`, the shape
/// core's `pageContextFields` recognizes): it runs only whole.
pub(crate) fn executable_call(map: &serde_json::Map<String, serde_json::Value>) -> bool {
    map.contains_key("tool") && map.contains_key("input")
}

pub(crate) struct PageContextFields {
    tool: ToolId,
    /// The declared page-context pointers, with the collection references.
    context_routes: CollectionRoutes,
    /// Each declared pointer's trie node and what a later part keeps there.
    kinds: BTreeMap<usize, PageContext>,
    pub(crate) collection_routes: CollectionRoutes,
    /// Declared collections at a fixed place in a row (no `*` on the way).
    pub(crate) fixed_paths: Vec<String>,
    /// Fixed collections and holders the schema lets a part omit.
    pub(crate) optional_paths: BTreeSet<String>,
}

impl PageContextFields {
    /// What a later part keeps of the member at row pointer `pointer`;
    /// `None`: the member rides the row's first part only.
    pub(crate) fn at(&self, pointer: &str) -> Option<PageContext> {
        self.context_routes
            .reach(pointer)
            .into_iter()
            .filter_map(|node| self.kinds.get(&node).copied())
            .max()
    }

    /// Whether the output schema requires the member at `pointer` on the
    /// object holding it, so every part keeps it.
    pub(crate) fn requires(&self, pointer: &str) -> bool {
        matches!(
            self.at(pointer),
            Some(PageContext::Required | PageContext::RequiredEvidence)
        )
    }

    /// Whether a later part keeps the member at `pointer`, and in what form:
    /// an identity, next or disclosure field rides whole; required evidence,
    /// which the first part delivered, stays only so the part validates, in
    /// the empty form the output schema admits there ([`emptiable_at`]): a
    /// free-text string as `""`, a list as `[]`, an object (not an
    /// executable call) with its members kept by this same rule. A value
    /// with no valid empty form (a number, an enum, a bounded string, list
    /// or record) rides whole: each part repeats it unchanged.
    pub(crate) fn keep_later(&self, pointer: &str, value: &mut serde_json::Value) -> bool {
        use serde_json::Value;
        match self.at(pointer) {
            None => false,
            Some(PageContext::Required | PageContext::Optional) => true,
            Some(PageContext::RequiredEvidence) => {
                if !emptiable_at(self.tool, pointer, value) {
                    return true;
                }
                match value {
                    Value::String(text) => text.clear(),
                    Value::Array(items) => items.clear(),
                    Value::Object(map) if !executable_call(map) => map.retain(|name, member| {
                        let member_pointer =
                            format!("{pointer}/{}", crate::json_pointer::escape(name));
                        self.keep_later(&member_pointer, member)
                    }),
                    _ => {}
                }
                true
            }
        }
    }

    /// Whether the value at `pointer` repeats whole on every part, itself or
    /// inside an identity, next or disclosure field: such a string is never
    /// cut into windows, since each part would repeat it.
    pub(crate) fn repeats_whole(&self, pointer: &str) -> bool {
        let mut prefix = pointer;
        while !prefix.is_empty() {
            if matches!(
                self.at(prefix),
                Some(PageContext::Required | PageContext::Optional)
            ) {
                return true;
            }
            prefix = prefix.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
        false
    }
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
        let declared = |kind: &str| {
            context[kind]
                .as_array()
                .expect("core declares each tool's page context")
                .iter()
                .map(|field| field.as_str().expect("page context pointer").to_owned())
                .collect::<Vec<_>>()
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
        let mut context_routes = CollectionRoutes::new(&[], &references);
        let mut kinds = BTreeMap::new();
        for (kind, context) in [
            ("optional", PageContext::Optional),
            ("requiredEvidence", PageContext::RequiredEvidence),
            ("required", PageContext::Required),
        ] {
            for pointer in declared(kind) {
                kinds.insert(context_routes.insert(&pointer), context);
            }
        }
        PageContextFields {
            tool,
            context_routes,
            kinds,
            collection_routes: CollectionRoutes::new(&paths, &references),
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

    /// Required evidence is emptied only where the output schema admits the
    /// empty form: a score question's levels (at least two items) and a
    /// choice question's labels (at least one property) repeat whole.
    /// `instructions` is optional beside a `noul` question's `criteria`
    /// (`clasify.noul-needs-one`), so the schema declares no repeat of it: a
    /// later part leaves it out.
    #[test]
    fn emptying_honours_the_output_schema_bounds() {
        use serde_json::json;
        let clasify = page_context_fields(ToolId::Clasify);
        let later = |pointer: &str, value: serde_json::Value| {
            let mut kept = value;
            assert!(clasify.keep_later(pointer, &mut kept), "{pointer}");
            kept
        };
        for (pointer, value) in [
            ("/data/questions/q/criteria", json!(["low", "high"])),
            ("/data/questions/q/criteria", json!({"yes": "holds"})),
        ] {
            assert_eq!(later(pointer, value.clone()), value, "{pointer}");
        }
        let mut instructions = json!("one named fact");
        assert!(
            !clasify.keep_later("/data/questions/q/instructions", &mut instructions),
            "an optional field the schema does not declare is not repeated"
        );
        // A section's byte pair and context are details fields, optional in
        // the schema: a later part leaves them out, they are stated once.
        for pointer in [
            "/data/resources/r/sections/0/target/bytes",
            "/data/resources/r/sections/0/context",
        ] {
            let mut kept = json!([0, 40]);
            assert!(!clasify.keep_later(pointer, &mut kept), "{pointer}");
        }
    }

    /// A later part keeps every required member, so it validates against the
    /// published output schema: for every tool and every required-evidence
    /// pointer, a string the schema leaves unbounded is kept as `""`, and one
    /// it bounds rides whole and unchanged.
    #[test]
    fn a_later_part_keeps_required_free_text_empty_for_every_tool() {
        let mut emptied = 0;
        for tool in ToolId::ALL {
            let contract = crate::contracts::tool_contract(tool).expect("embedded contract");
            let fields = page_context_fields(tool);
            for pointer in contract["pageContextFields"]["requiredEvidence"]
                .as_array()
                .expect("required evidence list")
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|pointer| pointer.replace('*', "0"))
            {
                let text = serde_json::json!("evidence text");
                let mut later = text.clone();
                let kept = fields.keep_later(&pointer, &mut later);
                assert!(kept, "{tool:?} {pointer}: a required member stays");
                if emptiable_at(tool, &pointer, &text) {
                    emptied += 1;
                    assert_eq!(later, "", "{tool:?} {pointer}: free text empties");
                } else {
                    assert!(kept, "{tool:?} {pointer}");
                    assert_eq!(later, text, "{tool:?} {pointer}: whole when bounded");
                }
            }
        }
        assert!(emptied > 0, "some required free text is declared");
    }

    #[test]
    fn every_tool_declares_each_page_context_pointer_once() {
        for tool in ToolId::ALL {
            let contract = crate::contracts::tool_contract(tool).expect("embedded contract");
            let declared = ["required", "requiredEvidence", "optional"]
                .iter()
                .flat_map(|kind| {
                    contract["pageContextFields"][*kind]
                        .as_array()
                        .expect("page context list")
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                })
                .collect::<Vec<_>>();
            let unique = declared.iter().collect::<std::collections::BTreeSet<_>>();
            assert_eq!(unique.len(), declared.len(), "{tool:?}");
            assert_eq!(
                page_context_fields(tool).kinds.len(),
                declared.len(),
                "{tool:?}"
            );
        }
    }

    /// Page context names a member of one object, not every member of that
    /// name: a related call's required `content` input leaves the row's
    /// optional `path` first-part evidence, and a recursive member resolves
    /// through its collection reference.
    #[test]
    fn page_context_resolves_by_row_pointer() {
        let fetch = page_context_fields(ToolId::LocalFetch);
        assert_eq!(
            fetch.at("/data/content"),
            Some(PageContext::RequiredEvidence)
        );
        assert_eq!(fetch.at("/data/path"), Some(PageContext::Optional));
        assert_eq!(fetch.at("/data/related/0/input/queries/0/content"), None);
        assert_eq!(fetch.at("/data/pagination"), Some(PageContext::Optional));
        let outline = page_context_fields(ToolId::AstSearch);
        assert_eq!(
            outline.at("/data/symbols/3/members/0/members/1/exported"),
            outline.at("/data/symbols/0/exported"),
        );
        assert_eq!(
            outline.at("/data/symbols/0/exported"),
            Some(PageContext::Optional)
        );
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
