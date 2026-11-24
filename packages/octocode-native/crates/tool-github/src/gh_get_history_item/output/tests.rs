use super::Output;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::response::output::{NOT_FOUND_HINT, ToolOutput};
use serde_json::{Value, json};

/// A history item is named by its repository and its number or ref, never
/// by a path: a missing one gets this tool's own recovery, not the shared
/// path-listing hint. Each operation's identifying field is named.
#[test]
fn a_missing_item_is_recovered_by_its_number_or_ref_not_a_path_listing() {
    for row in [
        json!({"operation": "pullRequest", "owner": "o", "repo": "r", "number": 1}),
        json!({"operation": "issue", "owner": "o", "repo": "r", "number": 1}),
        json!({"operation": "commit", "owner": "o", "repo": "r", "ref": "main"}),
        Value::Null,
    ] {
        let hint = Output
            .error_hint(ErrorCode::NotFound, &row)
            .expect("ghGetHistoryItem states its own notFound recovery");
        assert_ne!(hint, NOT_FOUND_HINT, "{row}");
        for field in ["owner", "repo", "number", "ref"] {
            assert!(hint.contains(field), "{field} missing from {hint:?}");
        }
        // The hint shows only on a row without related[]: it never names it.
        assert!(!hint.contains("related"), "{hint:?}");
    }
}
