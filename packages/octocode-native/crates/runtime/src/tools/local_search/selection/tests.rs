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
    // A hit with its one-line state charges about a hundred characters.
    runtime.limits.max_chars = max * 100;
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

/// A dense file no longer collapses into one unjudgeable group: a page judges
/// what its budget takes, and the rest stays unknown, reachable and countable.
#[test]
fn a_search_past_one_page_judges_what_fits_and_offers_the_rest() {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(root.path().join("large.txt"), "needle keep\n".repeat(3_000)).expect("source");
    let result = search(
        root.path(),
        json!({"contextLines":{"before":1,"after":1}}),
        None,
    )
    .expect("search");
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(receipt.unit, SelectionUnit::Hits);
    assert_eq!(receipt.total_items, 3_000);
    assert!(receipt.selected_items > 0 && receipt.unknown_items > 0);
    assert_eq!(receipt.selected_items + receipt.unknown_items, 3_000);
    let next = result.next.as_ref().unwrap();
    assert!(next.get("nextCandidatePage").is_some());
    assert!(next.get("unfiltered").is_some());
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
            "classification kept 0 of 2 hits that pass the classifiers. An empty classified result is not absence: next.unfiltered lists them all."
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
    assert_eq!(receipt.unit, SelectionUnit::Hits);
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
    // Every count states the kept hits: one file holds them. The two files
    // classification kept no hit of are said, with the call that lists them.
    assert_eq!(result.stats.matched_files, Some(1));
    assert!(
        result.warnings.iter().any(|note| note
            == "2 of 3 matched files kept no hit: next.unfiltered lists them."),
        "{:?}",
        result.warnings
    );
    let next = result.next.as_ref().expect("leads");
    let unknown = &next["unfilteredUnknown"]["input"]["queries"][0];
    assert!(unknown.get("include").is_none(), "{next}");
    assert!(unknown.get("selection").is_some(), "{next}");
    assert!(unknown.get("snapshot").is_some(), "{next}");
    let whole = &next["unfiltered"]["input"]["queries"][0];
    assert!(whole.get("include").is_none());
    assert!(whole.get("snapshot").is_some());
}

/// Selects `keep`, judges `near` related, excludes `drop`, else unknown.
struct SortingJudge;

impl SelectionJudge for SortingJudge {
    fn identity(&self) -> Value {
        json!({"test":"sorting"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| {
                let text = call.candidates[ordinal]
                    .state
                    .as_ref()
                    .map(|state| state["target"].to_string())
                    .unwrap_or_default();
                let decision = [
                    ("keep", Decision::Selected),
                    ("near", Decision::Related),
                    ("drop", Decision::Excluded),
                ]
                .into_iter()
                .find_map(|(word, decision)| text.contains(word).then_some(decision))
                .unwrap_or(Decision::Unknown);
                (ordinal, decision)
            })
            .collect())
    }
}

fn sorted_search(root: &std::path::Path, query: &LocalSearchQuery) -> LocalSearchResult {
    sorted_search_with(root, query, None)
}

/// [`sorted_search`] judging at most `max` hits' worth of states per call.
fn sorted_search_with(
    root: &std::path::Path,
    query: &LocalSearchQuery,
    max: Option<usize>,
) -> LocalSearchResult {
    let policy = crate::tools::test_support::workspace_policy(root);
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let mut selection = selection_fake::runtime(
        SortingJudge,
        executor.handle().clone(),
        Duration::from_secs(5),
    );
    if let Some(max) = max {
        // A hit with its one-line state charges about a hundred characters.
        selection.limits.max_chars = max * 100;
    }
    execute_local_search(
        query,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        Some(&selection),
    )
    .expect("search")
}

fn hit_lines(result: &LocalSearchResult) -> Vec<(String, u32)> {
    result
        .files
        .iter()
        .flat_map(|file| {
            file.matches
                .iter()
                .flatten()
                .map(|hit| (file.path.clone(), hit.line))
        })
        .collect()
}

/// `next.unfilteredUnknown` replays the selection's undecided view over the
/// same scan: walking every page of it reaches exactly the hits judged
/// unsure, once each, and none that were kept, related or excluded, nor the
/// rest of a file that holds one.
#[test]
fn walking_the_unknown_lead_reaches_exactly_the_undecided_hits_once() {
    // Each hit 21 lines from the next, so the default classification
    // context gives every hit a window of its own.
    let spaced = |hits: &[&str]| hits.join(&format!("\n{}", "x\n".repeat(20))) + "\n";
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(
        root.path().join("a.rs"),
        spaced(&[
            "needle keep one",
            "needle maybe one",
            "needle near one",
            "needle drop one",
            "needle maybe two",
        ]),
    )
    .expect("a");
    std::fs::write(root.path().join("b.rs"), "needle maybe three\n").expect("b");
    std::fs::write(
        root.path().join("c.rs"),
        spaced(&["needle keep two", "needle drop two"]),
    )
    .expect("c");
    let first: LocalSearchQuery = serde_json::from_value(json!({
        "path": root.path(), "matchString": ["needle"], "limit": 1,
        "classification": {"predicates": ["Is this kept?"]},
    }))
    .expect("query");
    let result = sorted_search(root.path(), &first);
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(
        (
            receipt.selected_items,
            receipt.excluded_items,
            receipt.unknown_items
        ),
        (2, 2, 3)
    );
    let lead = &result.next.as_ref().expect("leads")["unfilteredUnknown"]["input"]["queries"][0];
    let mut query: LocalSearchQuery =
        serde_json::from_value(lead.clone()).expect("executable lead");
    let mut reached = Vec::new();
    for pages in 1.. {
        assert!(pages < 20, "the unknown lead never ends");
        let page = sorted_search(root.path(), &query);
        let row = page.to_row().expect("row");
        assert!(
            row.get("selection").is_none(),
            "an unfiltered listing: {row}"
        );
        for channel in ["unfiltered", "unfilteredUnknown", "nextCandidatePage"] {
            assert!(row["next"].get(channel).is_none(), "{channel}: {row}");
        }
        // Selection notes are said on the first page.
        assert!(
            pages > 1
                || page.warnings.iter().any(|note| note
                    .contains("Lists only the 3 hits the selection's walk left undecided")),
            "{:?}",
            page.warnings
        );
        reached.extend(hit_lines(&page));
        let next = row["next"]["nextPage"]["input"]["queries"][0].clone();
        if !next.is_object() {
            break;
        }
        query = serde_json::from_value(next).expect("next page");
    }
    reached.sort();
    let undecided =
        [("a.rs", 22), ("a.rs", 85), ("b.rs", 1)].map(|(path, line)| (path.to_owned(), line));
    assert_eq!(reached, undecided);
}

/// Every page of a candidate chain counts the whole walk's unknown hits, and
/// its `unfilteredUnknown` lead lists that same scope: walking the last
/// page's lead reaches each page's undecided hit once, none still pending.
/// The leads are followed as emitted, under an explicit zero context.
#[test]
fn the_unknown_lead_of_a_candidate_chain_covers_the_whole_walk() {
    let root = tempfile::tempdir().expect("fixture");
    for (name, text) in [
        ("a.rs", "needle maybe one\nx\nx\nneedle keep one\n"),
        ("b.rs", "needle maybe two\nx\nx\nneedle drop\n"),
        ("c.rs", "needle maybe three\nx\nx\nneedle near\n"),
    ] {
        std::fs::write(root.path().join(name), text).expect("fixture");
    }
    let mut query: LocalSearchQuery = serde_json::from_value(json!({
        "path": root.path(), "matchString": ["needle"],
        "contextLines": {"before": 0, "after": 0},
        "classification": {"predicates": ["Is this kept?"]},
    }))
    .expect("query");
    let lead = |result: &LocalSearchResult, channel: &str| {
        let row = result.next.as_ref()?.get(channel)?["input"]["queries"][0].clone();
        Some(serde_json::from_value::<LocalSearchQuery>(row).expect("executable lead"))
    };
    let (mut pages, mut unknown) = (0, Vec::new());
    let last = loop {
        pages += 1;
        assert!(pages < 10, "the chain ends");
        let result = sorted_search_with(root.path(), &query, Some(2));
        unknown.push(result.selection.as_ref().expect("receipt").unknown_items);
        match lead(&result, "nextCandidatePage") {
            Some(next) => query = next,
            None => break result,
        }
    };
    assert!(pages >= 3, "a chain of candidate pages: {unknown:?}");
    assert_eq!(unknown.last(), Some(&3), "{unknown:?}");
    let mut view = lead(&last, "unfilteredUnknown").expect("the unknown lead");
    let mut reached = Vec::new();
    for page in 1.. {
        assert!(page < 20, "the unknown lead never ends");
        let result = sorted_search(root.path(), &view);
        assert!(
            page > 1
                || result.warnings.iter().any(|note| note
                    .contains("Lists only the 3 hits the selection's walk left undecided")),
            "{:?}",
            result.warnings
        );
        reached.extend(hit_lines(&result));
        let row = result.to_row().expect("row");
        let next = row["next"]["nextPage"]["input"]["queries"][0].clone();
        if !next.is_object() {
            break;
        }
        view = serde_json::from_value(next).expect("next page");
    }
    reached.sort();
    let undecided =
        [("a.rs", 1), ("b.rs", 1), ("c.rs", 1)].map(|(path, line)| (path.to_owned(), line));
    assert_eq!(reached, undecided);
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
    assert_eq!(totals, [5, 5, 5], "each page counts the whole walk");
    seen.sort();
    assert_eq!(seen, expected, "every kept hit once, none twice");
}

/// With no context field the contract default `contextLines: {before: 0,
/// after: 0}` applies, and the candidate page omits it as well: replaying
/// that page binds the same plan as the first call, whose context was
/// implied.
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
    assert_eq!(totals, [4, 4], "each page counts the whole walk");
}

/// A scout whose evaluations the default window refused and the
/// long-context window answered says so on its row, with the tokens billed.
#[test]
fn a_long_context_retry_is_disclosed_on_the_scout_row() {
    use crate::tools::selection::long_window_fake::{BILLED, LongWindow};
    let root = fixture();
    let window = LongWindow::start();
    let query: LocalSearchQuery = serde_json::from_value(json!({
        "path": root.path(), "matchString": ["needle"],
        "classification": {"predicates": ["Is this kept?"]},
    }))
    .expect("query");
    let result = execute_local_search(
        &query,
        &crate::tools::test_support::workspace_policy(root.path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        Some(&window.selection),
    )
    .expect("search");
    let retried = u64::try_from(window.long_requests()).expect("count");
    assert!(retried > 0, "the provider answered in the long window");
    let notice = result
        .warnings
        .iter()
        .find(|note| note.contains("long-context window"))
        .unwrap_or_else(|| panic!("no retry notice: {:?}", result.warnings));
    let evaluations = octocode_engine::text::counted(retried, "evaluation", "evaluations");
    assert!(notice.starts_with(&evaluations), "{notice}");
    assert!(
        notice.contains(&format!("{} input tokens billed", retried * BILLED)),
        "{notice}"
    );
    assert!(
        notice.contains("longContext:false refuses instead"),
        "{notice}"
    );
}

mod candidates;
mod corpus;
mod failures;
