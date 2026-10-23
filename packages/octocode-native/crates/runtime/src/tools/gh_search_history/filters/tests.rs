//! Tests of the qualifiers parse into typed filters.
use super::QUALIFIER_FIELDS;

/// The `qualifiers` keys the contract declares for `operation`.
fn contract_keys(operation: &str) -> Vec<&'static str> {
    crate::contracts::query_schema_value(
        crate::id::ToolId::GhSearchHistory,
        Some(operation),
        "qualifiers",
        "x-qualifierKeys",
    )
    .and_then(serde_json::Value::as_array)
    .map_or(&[][..], Vec::as_slice)
    .iter()
    .filter_map(serde_json::Value::as_str)
    .collect()
}

/// Every key the contract declares for an operation sets a native
/// filter, and every native filter key is one the contract declares.
#[test]
fn every_contract_qualifier_key_sets_a_native_filter() {
    let mut declared = Vec::new();
    for operation in ["pullRequest", "issue", "commit"] {
        let keys = contract_keys(operation);
        assert!(!keys.is_empty(), "{operation}: no x-qualifierKeys");
        for key in keys {
            assert!(
                QUALIFIER_FIELDS.iter().any(|(name, _)| *name == key),
                "{operation}: contract key {key} has no native filter"
            );
            declared.push(key);
        }
    }
    for (name, _) in QUALIFIER_FIELDS {
        assert!(declared.contains(name), "{name}: no operation declares it");
    }
}

/// Which operation takes a key is the typed query's (the contract's)
/// check: a key outside the operation's list never reaches the parser.
#[test]
fn a_key_outside_the_operations_list_is_rejected_by_the_typed_query() {
    for (operation, qualifiers) in [
        ("issue", "review:approved"),
        ("commit", "label:bug"),
        ("pullRequest", "committer:dev"),
        ("pullRequest", "repo:evil/x"),
    ] {
        let row = serde_json::json!({"operation":operation,"owner":"o","repo":"r",
            "qualifiers":qualifiers});
        assert!(
            serde_json::from_value::<crate::tools::gh_search_history::GhSearchHistoryQuery>(row)
                .is_err(),
            "{operation} {qualifiers}"
        );
    }
}
