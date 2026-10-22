//! What the shared response stages ask about structureSearch output.

use crate::contracts::listing_roles::structure_search::FIELDS;
use crate::response::output::{PathAnchor, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;

/// Query fields that narrow a listing (contract `x-listing`, both
/// operations').
fn filters() -> impl Iterator<Item = &'static str> {
    FIELDS
        .iter()
        .filter(|(_, role)| role.filters())
        .map(|(field, _)| *field)
}

/// The hint of a narrowed listing: every filter it may loosen, by name,
/// within `MAX_GUIDANCE_CHARS`.
static FILTER_HINT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "Loosen {}, or widen maxDepth.",
        filters().collect::<Vec<_>>().join("/")
    )
});

impl ToolOutput for Output {
    fn fallback_hint(&self, query: &Value) -> &'static str {
        use crate::response::output::has_value;
        if filters().any(|key| has_value(query, key)) {
            FILTER_HINT.as_str()
        } else {
            "Check path with its parent tree, or raise maxDepth."
        }
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "exact"
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::QueryParent
    }
    fn reads_worktree(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_filter_field_gets_the_filter_hint() {
        let narrowed = "Loosen access/empty/entryType/exclude/extensions/include/minDepth/nameRegex/permissions/size/time, or widen maxDepth.";
        assert_eq!(FILTER_HINT.as_str(), narrowed);
        assert!(narrowed.chars().count() <= crate::response::rows::MAX_GUIDANCE_CHARS);
        for filter in filters() {
            let query = json!({"operation": "files", "path": ".", filter: "x"});
            assert_eq!(Output.fallback_hint(&query), narrowed, "{filter}");
        }
        // A field that only scopes the walk loosens nothing.
        for (key, value) in [("maxDepth", json!(2)), ("includeHidden", json!(true))] {
            let query = json!({"operation": "files", "path": ".", key: value});
            assert_eq!(
                Output.fallback_hint(&query),
                "Check path with its parent tree, or raise maxDepth.",
                "{key}"
            );
        }
        assert_eq!(
            Output.fallback_hint(&json!({"operation": "files", "path": "."})),
            "Check path with its parent tree, or raise maxDepth."
        );
    }
}
