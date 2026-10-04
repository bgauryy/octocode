//! Default projection shared by the GitHub tools (part of
//! [`super::response::minimize_row`], so tool-internal reads such as clasify
//! contexts keep the full rows): a result
//! page keeps only the pagination facts its `next.*` does not carry, and rows
//! do not restate the caller's ref or path or count their own entries.
//! `debug: true` keeps everything.
use crate::tools::id::ToolId;
use serde_json::{Map, Value};

/// Compacts one GitHub tool's data object in place.
pub(super) fn compact(tool: ToolId, data: &mut Value, query: &Value) {
    if query.get("debug").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let Some(fields) = data.as_object_mut() else {
        return;
    };
    slim_pagination(fields);
    if tool == ToolId::GhSearchHistory
        && fields.get("path").is_some()
        && fields.get("path") == query.get("path")
    {
        fields.remove("path");
    }
    // The resolved branch is next-call input when the caller named none;
    // it goes only when it repeats the requested branch.
    if tool == ToolId::GhStructure
        && fields.get("resolvedBranch").is_some()
        && fields.get("resolvedBranch") == query.get("branch")
    {
        fields.remove("resolvedBranch");
    }
}

fn is_continuation(map: &Map<String, Value>) -> bool {
    map.get("tool").is_some_and(Value::is_string) && map.get("query").is_some_and(Value::is_object)
}

/// A search page's `pagination` keeps whether more exists and how many
/// matched. The page size echoes the request, page one is the default, and
/// the next page number rides `next.nextPage`; a false cap flag asserts
/// nothing, and a true one repeats `partialReasons`/`providerLimit`.
fn slim_pagination(data: &mut Map<String, Value>) {
    let has_next_page = data
        .get("next")
        .and_then(|next| next.get("nextPage"))
        .and_then(Value::as_object)
        .is_some_and(is_continuation);
    let limited = data.contains_key("providerLimit");
    let Some(page) = data.get_mut("pagination").and_then(Value::as_object_mut) else {
        return;
    };
    page.remove("perPage");
    if page.get("currentPage").and_then(Value::as_u64) == Some(1) {
        page.remove("currentPage");
    }
    if has_next_page || page.get("nextPage").is_some_and(Value::is_null) {
        page.remove("nextPage");
    }
    match page.get("totalMatchesCapped").and_then(Value::as_bool) {
        Some(false) => {
            page.remove("totalMatchesCapped");
        }
        Some(true) if limited => {
            page.remove("totalMatchesCapped");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pagination_keeps_what_next_does_not_carry() {
        let mut data = json!({
            "pagination": {"currentPage":1,"totalPages":2,"perPage":5,"totalMatches":7,
                "totalMatchesCapped":false,"hasMore":true,"nextPage":2},
            "next": {"nextPage": {"tool":"ghSearchCode","query":{"page":2}}}
        });
        compact(ToolId::GhSearchCode, &mut data, &json!({}));
        assert_eq!(
            data["pagination"],
            json!({"totalPages":2,"totalMatches":7,"hasMore":true})
        );
        let mut capped = json!({
            "pagination": {"totalMatches":1000,"totalMatchesCapped":true,"hasMore":true,"currentPage":3},
            "providerLimit": {"reason":"providerResultCap","maxResults":1000}
        });
        compact(ToolId::GhSearchRepo, &mut capped, &json!({}));
        assert_eq!(
            capped["pagination"],
            json!({"totalMatches":1000,"hasMore":true,"currentPage":3})
        );
    }

    /// Row data after the verbose stage (a default, non-debug row).
    fn pruned(tool: ToolId, data: &Value) -> Value {
        let mut row = json!({"index":0,"data":data});
        super::super::verbose::prune_row(&mut row, tool, &json!({}));
        row["data"].take()
    }

    #[test]
    fn tree_listings_do_not_restate_the_ref_or_count_their_rows() {
        let listing = json!({
            "structure": [{"dir":"src","files":["a.rs"]}],
            "summary": {"totalFiles":1,"totalFolders":0,"pattern":"**/a.rs"},
            "resolvedBranch": "63c5760d8a672cee96e1e523d84bfa1c77d9ee4c"
        });
        // `summary` is verbose (core field class); the verbose stage drops it.
        let mut pinned = pruned(ToolId::GhStructure, &listing);
        compact(
            ToolId::GhStructure,
            &mut pinned,
            &json!({"branch":"63c5760d8a672cee96e1e523d84bfa1c77d9ee4c"}),
        );
        assert_eq!(
            pinned,
            json!({"structure": [{"dir":"src","files":["a.rs"]}]})
        );
        // A resolved default branch is news to the caller.
        let mut default_branch = listing;
        compact(ToolId::GhStructure, &mut default_branch, &json!({}));
        assert_eq!(
            default_branch["resolvedBranch"],
            "63c5760d8a672cee96e1e523d84bfa1c77d9ee4c"
        );
    }

    #[test]
    fn requested_paths_and_file_classes_are_not_restated() {
        let mut history = json!({"type":"file","path":"fastapi/routing.py","commits":[]});
        compact(
            ToolId::GhSearchHistory,
            &mut history,
            &json!({"path":"fastapi/routing.py"}),
        );
        assert_eq!(history, json!({"type":"file","commits":[]}));
        let file = json!({"files":[{"path":"a.rs","content":"1\tx\n","totalLines":9,
            "sourceChars":40,"fileType":"code"}]});
        let mut file = pruned(ToolId::GhGetFileContent, &file);
        compact(ToolId::GhGetFileContent, &mut file, &json!({}));
        assert_eq!(
            file,
            json!({"files":[{"path":"a.rs","content":"1\tx\n","totalLines":9}]})
        );
    }

    #[test]
    fn debug_keeps_everything() {
        let original = json!({
            "pagination": {"currentPage":1,"perPage":5,"hasMore":true,"nextPage":2},
            "next": {"nextPage": {"tool":"ghSearchCode","confidence":"exact","query":{"page":2}}}
        });
        let mut data = original.clone();
        compact(ToolId::GhSearchCode, &mut data, &json!({"debug": true}));
        assert_eq!(data, original);
    }
}
