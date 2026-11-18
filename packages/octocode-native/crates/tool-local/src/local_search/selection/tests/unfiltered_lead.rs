//! `next.unfiltered` of a classified search is the same search without
//! classification: following it lists every hit, kept or not.

use super::*;

/// The lead's query row, as the caller runs it.
fn lead(result: &LocalSearchResult) -> LocalSearchQuery {
    let row = &result.next.as_ref().expect("leads")["unfiltered"]["input"]["queries"][0];
    serde_json::from_value(row.clone()).expect("an unfiltered lead is a localSearch row")
}

/// The search `query` runs without a selection runtime, as a call that
/// names no classification does.
fn unclassified(
    root: &std::path::Path,
    query: &LocalSearchQuery,
) -> Result<LocalSearchResult, ToolError> {
    execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        query,
        &octocode_runtime_base::test_support::workspace_policy(root),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
    )
}

/// `(path, hit count)` of each listed file, by path.
fn listed(result: &LocalSearchResult) -> Vec<(String, usize)> {
    let mut files = result
        .files
        .iter()
        .map(|file| {
            let name = std::path::Path::new(&file.path)
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            (name, file.matches.as_ref().map_or(0, Vec::len))
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// A search in walk order (the default sort) streams once it is
/// unclassified: the lead lists every matched file, the ones classification
/// kept no hit of included.
#[test]
fn following_next_unfiltered_of_a_traversal_search_lists_every_hit_unclassified() {
    let root = fixture();
    let classified = search(root.path(), json!({}), None).expect("classified search");
    assert_eq!(classified.files.len(), 1, "classification left hits out");
    let all = unclassified(root.path(), &lead(&classified)).expect("the lead runs");
    assert_eq!(
        listed(&all),
        [
            ("a.rs".to_owned(), 3),
            ("b.rs".to_owned(), 1),
            ("c.rs".to_owned(), 1)
        ]
    );
    assert!(
        all.selection.is_none(),
        "an unfiltered listing carries no receipt"
    );
}

/// A sorted search collects its scan whole either way; its lead lists every
/// hit too.
#[test]
fn following_next_unfiltered_of_a_sorted_search_lists_every_hit_unclassified() {
    let root = fixture();
    let classified = search(
        root.path(),
        json_fixture(r#"{"sort": {"by": "path", "order": "asc"}}"#),
        None,
    )
    .expect("classified search");
    let all = unclassified(root.path(), &lead(&classified)).expect("the lead runs");
    assert_eq!(
        listed(&all),
        [
            ("a.rs".to_owned(), 3),
            ("b.rs".to_owned(), 1),
            ("c.rs".to_owned(), 1)
        ]
    );
}

/// The lead is a fresh request: none of the private progress a cursor
/// saves (a page, a snapshot, a selection plan) rides it.
#[test]
fn the_unfiltered_lead_holds_no_private_progress() {
    let root = fixture();
    let classified = search(root.path(), json!({}), None).expect("classified search");
    let row = &classified.next.as_ref().expect("leads")["unfiltered"]["input"]["queries"][0];
    let private = octocode_runtime_base::contracts::state_fields(ToolId::LocalSearch);
    let carried = private
        .iter()
        .filter(|field| row.get(field.as_str()).is_some())
        .collect::<Vec<_>>();
    assert!(carried.is_empty(), "{carried:?} in {row}");
    assert!(row.get("classification").is_none(), "{row}");
}
