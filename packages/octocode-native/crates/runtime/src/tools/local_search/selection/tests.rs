use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use crate::tools::local_search::{LocalSearchResult, SearchStatus, execute_local_search};
use crate::tools::selection::{JudgeCall, Judgments, SelectionJudge};
use crate::tools::test_support::selection_fake::{self, DOWN, REFUSED, WordJudge};
use octocode_contract_types::{ErrorCode, LocalSearchQuery};
use std::time::Duration;

fn runtime(fail: Option<ErrorCode>, executor: &tokio::runtime::Runtime) -> Runtime {
    runtime_with(fail, executor, 64)
}

fn runtime_with(
    fail: Option<ErrorCode>,
    executor: &tokio::runtime::Runtime,
    max: usize,
) -> Runtime {
    let mut runtime = selection_fake::runtime(
        WordJudge { fail },
        executor.handle().clone(),
        Duration::from_secs(5),
    );
    runtime.limits.max_candidates = max;
    runtime
}

fn search(
    root: &std::path::Path,
    fields: Value,
    fail: Option<ErrorCode>,
) -> Result<LocalSearchResult, ToolError> {
    search_with(root, fields, fail, 64)
}

fn search_with(
    root: &std::path::Path,
    fields: Value,
    fail: Option<ErrorCode>,
    max: usize,
) -> Result<LocalSearchResult, ToolError> {
    // Bare hit lines (`contextLines: {before: 0, after: 0}`) unless a test asks for context:
    // these tests count one judgment per hit, which the classification context
    // default would merge into shared windows.
    let mut query = json!({"path": root, "matchString": ["needle"],
            "contextLines": {"before":0,"after":0}, "classification": {"predicates": ["Is this kept?"]}});
    for (key, value) in fields.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    if query["view"] == "matchOnly" {
        query.as_object_mut().expect("query").remove("contextLines");
    }
    let query: LocalSearchQuery = serde_json::from_value(query).expect("query");
    let policy = crate::tools::test_support::workspace_policy(root);
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let selection = runtime_with(fail, &executor, max);
    execute_local_search(
        &query,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        Some(&selection),
    )
}

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(
        root.path().join("a.rs"),
        "needle keep one\nx\nx\nneedle drop\nx\nx\nneedle keep two\n",
    )
    .expect("a");
    std::fs::write(root.path().join("b.rs"), "needle maybe\n").expect("b");
    std::fs::write(root.path().join("c.rs"), "needle drop\n").expect("c");
    root
}

#[test]
fn span_selection_judges_each_occurrence_without_context() {
    let root = fixture();
    let result = search(
        root.path(),
        json!({"matchString":["needle (keep|drop)"],"regexEngine":"rust","view":"matchOnly"}),
        None,
    )
    .expect("search");
    let kept = result
        .files
        .iter()
        .flat_map(|file| file.matches.iter().flatten())
        .map(|hit| hit.value.as_str())
        .collect::<Vec<_>>();
    assert_eq!(kept, ["needle keep", "needle keep"]);
    let receipt = result.selection.expect("receipt");
    assert_eq!((receipt.selected_items, receipt.excluded_items), (2, 2));
}

#[test]
fn oversized_unknown_group_does_not_claim_its_hits_were_judged() {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(
        root.path().join("large.txt"),
        "needle keep\n".repeat(30_000),
    )
    .expect("source");
    let result = search(
        root.path(),
        json!({"contextLines":{"before":1,"after":1}}),
        None,
    )
    .expect("search");
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(receipt.unknown_items, 1);
    assert_eq!(receipt.selected_items, 0);
    assert!(
        result
            .warnings
            .iter()
            .any(|note| note.contains("1 could not be judged, so none of the 30000 hit rows"))
    );
    assert!(
        result
            .warnings
            .iter()
            .all(|note| !note.contains("judged hit rows"))
    );
    assert!(result.next.as_ref().unwrap().get("unfiltered").is_some());
}

#[test]
fn a_selection_that_keeps_no_judged_hit_is_not_absence() {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(root.path().join("a.rs"), "needle drop\nx\nneedle drop\n").expect("a");
    let result = search(root.path(), json!({}), None).expect("search");
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!((receipt.selected_items, receipt.unknown_items), (0, 0));
    assert!(
        result.warnings.iter().any(|note| note.contains(
            "every chunk was judged and none passed the questions, so none of the 2 hit rows (1 files) is listed. An empty classified result is not absence: the unfiltered search matched those 2 rows, and `next.unfiltered` lists them."
        )),
        "{:?}",
        result.warnings
    );
    assert!(
        result
            .warnings
            .iter()
            .all(|note| !note.contains("the answer")),
        "{:?}",
        result.warnings
    );
    assert!(result.next.as_ref().unwrap().get("unfiltered").is_some());
}

#[test]
fn each_hit_is_judged_and_counts_describe_the_kept_hits() {
    let root = fixture();
    let result = search(root.path(), json!({}), None).expect("selected search");
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(receipt.unit, SelectionUnit::Chunks);
    assert_eq!(
        (
            receipt.total_items,
            receipt.selected_items,
            receipt.excluded_items,
            receipt.unknown_items
        ),
        (5, 2, 2, 1)
    );
    assert_eq!(result.files.len(), 1, "{:?}", result.files);
    let kept = result.files[0].matches.as_ref().expect("rows");
    assert_eq!(
        kept.iter().map(|hit| hit.line).collect::<Vec<_>>(),
        [1, 7],
        "the excluded hit in a kept file is dropped"
    );
    assert_eq!(result.stats.total_occurrences, Some(2));
    assert_eq!(result.stats.matched_lines, Some(2));
    let next = result.next.as_ref().expect("leads");
    let unknown = &next["unfilteredUnknown"]["input"]["queries"][0];
    assert_eq!(unknown["include"], json!(["/b.rs"]), "{next}");
    assert!(unknown.get("classification").is_none());
    assert!(unknown.get("snapshot").is_none());
    let whole = &next["unfiltered"]["input"]["queries"][0];
    assert!(whole.get("include").is_none());
    assert!(whole.get("snapshot").is_some());
}

#[test]
fn the_unknown_lead_reaches_exactly_the_files_holding_unknown_hits() {
    let root = fixture();
    let result = search(root.path(), json!({}), None).expect("selected search");
    let lead = result.next.expect("leads")["unfilteredUnknown"]["input"]["queries"][0].clone();
    let query: LocalSearchQuery = serde_json::from_value(lead).expect("executable lead");
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let unfiltered = super::super::execute_local_search(
        &query,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
    )
    .expect("unknown evidence");
    let paths = unfiltered
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["b.rs"]);
}

/// Groups past the candidate budget, including the rest of a file the
/// budget reached in part, are judged by `nextCandidatePage`: the same
/// classified search of the same scan. Following the chain judges every
/// group exactly once and shows exactly the hits one unbudgeted call
/// keeps.
#[test]
fn a_chain_of_candidate_pages_judges_every_group_once() {
    let root = fixture();
    let shown = |result: &LocalSearchResult| {
        result
            .files
            .iter()
            .flat_map(|file| {
                let path = file.path.clone();
                file.matches
                    .iter()
                    .flatten()
                    .map(move |hit| (path.clone(), hit.line))
            })
            .collect::<Vec<_>>()
    };
    let unbudgeted = search(root.path(), json!({}), None).expect("one call");
    let mut expected = shown(&unbudgeted);
    expected.sort();
    // a.rs holds groups 0-2, b.rs 3, c.rs 4; two are judged per call.
    let mut result = search_with(root.path(), json!({}), None, 2).expect("first page");
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(receipt.reason.as_deref(), Some("candidateBudget"));
    assert!(
        !result
            .warnings
            .iter()
            .any(|w| w.contains("were not judged")),
        "{:?}",
        result.warnings
    );
    let (mut totals, mut seen) = (Vec::new(), Vec::new());
    loop {
        let receipt = result.selection.as_ref().expect("receipt");
        totals.push(receipt.total_items);
        seen.extend(shown(&result));
        let Some(page) = result
            .next
            .as_ref()
            .and_then(|next| next.get("nextCandidatePage"))
        else {
            break;
        };
        let row = page["input"]["queries"][0].clone();
        assert!(row.get("classification").is_some(), "classified: {row}");
        assert!(row.get("snapshot").is_some() && row.get("selection").is_some());
        assert!(row.get("include").is_none(), "the same scan: {row}");
        assert!(totals.len() < 5, "the chain ends");
        result = search_with(root.path(), row, None, 2).expect("next candidates");
    }
    assert_eq!(totals, [5, 3, 1], "each page counts the groups it judges");
    seen.sort();
    assert_eq!(seen, expected, "every kept hit once, none twice");
}

/// With no context field the classification context default applies, and the
/// candidate page states it: replaying that page binds the same plan as
/// the first call, whose context was implied.
#[test]
fn a_candidate_page_chains_under_the_default_classification_context() {
    let root = tempfile::tempdir().expect("fixture");
    let gap = "x\n".repeat(20);
    std::fs::write(
        root.path().join("a.rs"),
        format!("needle keep one\n{gap}needle drop\n{gap}needle keep two\n"),
    )
    .expect("a");
    std::fs::write(root.path().join("b.rs"), "needle keep\n").expect("b");
    let unset = json!({"contextLines": null});
    let mut result = search_with(root.path(), unset, None, 2).expect("first page");
    let mut totals = Vec::new();
    while let Some(page) = result
        .next
        .as_ref()
        .and_then(|next| next.get("nextCandidatePage"))
    {
        totals.push(result.selection.as_ref().expect("receipt").total_items);
        assert!(totals.len() < 4, "the chain ends");
        let mut row = page["input"]["queries"][0].clone();
        // The page as sent: without a stated context the helper's
        // `contextLines: {before: 0, after: 0}` must not fill it in.
        row.as_object_mut()
            .expect("row")
            .entry("contextLines")
            .or_insert(Value::Null);
        result = search_with(root.path(), row, None, 2).expect("the candidate page replays");
    }
    totals.push(result.selection.as_ref().expect("receipt").total_items);
    assert_eq!(totals, [4, 2], "each page judges the groups left");
}

mod candidates;
mod failures;
