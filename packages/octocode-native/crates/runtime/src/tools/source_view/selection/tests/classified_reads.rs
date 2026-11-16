//! Classified reads: recovery leads, window room, and candidate pages.
use super::*;
use octocode_contract_types::LocalFetchQuery;

#[test]
fn context_selection_omits_unrelated_bodies_and_exposes_recovery() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::source_view::SourceFacts;
    struct Relevance;
    impl SelectionJudge for Relevance {
        fn identity(&self) -> serde_json::Value {
            json!({"test":"selection"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&i| {
                    let body = call.candidates[i].state.as_ref().unwrap()["content"]
                        .as_str()
                        .unwrap();
                    (
                        i,
                        if body.contains("REQUIRED_EVIDENCE") {
                            Decision::Selected
                        } else {
                            Decision::Excluded
                        },
                    )
                })
                .collect())
        }
    }
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.limits.max_chars = 20_000;
    runtime.judge = Arc::new(Relevance);
    let text = format!(
        "UNRELATED_SENTINEL {}\nREQUIRED_EVIDENCE retry stops after three attempts\n",
        "x".repeat(2_030)
    );
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"source.txt",
        "classification":{"predicates":["When do retries stop?"]}
    }))
    .unwrap();
    let result = crate::tools::source_view::process_fetched_content_selected(
        &query,
        text.as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    );
    let body = result.content.as_deref().unwrap();
    assert!(body.contains("REQUIRED_EVIDENCE"));
    assert!(
        !body.contains("UNRELATED_SENTINEL"),
        "Unrelated bodies must remain outside host context"
    );
    let value = serde_json::to_value(&result).unwrap();
    assert!(
        value.pointer("/next/unfiltered").is_some(),
        "Withheld evidence must be explicitly recoverable"
    );
}

/// A classified page leaves room in its window for what rides beside
/// its text (plan token, receipt, both unfiltered leads): the same read
/// cut for the same window returns less text than an unclassified one,
/// by at least those chars.
#[test]
fn a_classified_page_leaves_room_for_its_selection_fields() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::source_view::SourceFacts;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail: None });
    runtime.limits.max_chars = 1_000_000;
    let text = (0..400)
        .map(|n| {
            let word = if n % 7 == 3 { "maybe" } else { "keep" };
            format!("{word} {n} {}\n", "y".repeat(60))
        })
        .collect::<String>();
    let facts = SourceFacts {
        window: Some(8_000),
        ..SourceFacts::default()
    };
    let read = |classified: bool| {
        let mut query = json!({"path":"source.txt"});
        if classified {
            query["classification"] = json!({"predicates":["Is it kept?"]});
        }
        let query: LocalFetchQuery = serde_json::from_value(query).unwrap();
        crate::tools::source_view::process_fetched_content_selected(
            &query,
            text.as_bytes(),
            Path::new("source.txt"),
            "source.txt",
            &facts,
            &ContentSecurity::new(),
            &NeverCancel,
            classified.then_some(&runtime),
        )
    };
    let plain = read(false);
    let classified = read(true);
    assert!(classified.selection.is_some(), "{classified:?}");
    let chars = |result: &crate::tools::source_view::LocalFetchResult| {
        result.content.as_deref().map_or(0, str::len)
    };
    let beside = serde_json::to_value(&classified)
        .map(|wire| {
            let mut extras = 0;
            for key in ["selection", "next"] {
                extras += octocode_engine::json_size::utf16_units(&wire[key]).unwrap_or(0);
            }
            extras
        })
        .unwrap_or(0);
    assert!(beside > 0);
    assert!(
        chars(&classified) + beside <= chars(&plain) + 400,
        "classified {} + beside {beside} vs plain {}",
        chars(&classified),
        chars(&plain)
    );
}

#[test]
fn unfiltered_unknown_reads_exactly_the_unknown_chunks_beside_the_whole_view() {
    let result = word_read(None);
    // The filtered-evidence notice is the response stage's, said once
    // (`selection_errors::decorate`), not the reader's.
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| warning.contains("not proof of absence")),
        "{:?}",
        result.warnings
    );
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(
        (
            receipt.selected_items,
            receipt.excluded_items,
            receipt.unknown_items
        ),
        (2, 1, 2)
    );
    let next = result.next.expect("recovery");
    // The unknown chunks are the same read replaying the selection's
    // undecided view, not a rewritten `ranges` read.
    let unknown = next.unfiltered_unknown.expect("unknown chunks").query;
    assert!(local_fetch_query::has_classification(&unknown));
    assert!(local_fetch_query::selection(&unknown).is_some());
    assert!(local_fetch_query::line_ranges(&unknown).is_empty());
    let whole = next.unfiltered.expect("whole view").query;
    assert!(!local_fetch_query::has_classification(&whole));
    assert!(local_fetch_query::line_ranges(&whole).is_empty());
}

/// Chunks past the candidate budget are judged by `nextCandidatePage`,
/// the same classified read; the first page says how many were judged.
/// Following the chain judges every chunk once and selects exactly what
/// one unbudgeted read selects.
#[test]
fn a_chain_of_candidate_pages_judges_every_chunk_once() {
    let words = [
        "keep", "drop", "maybe", "keep", "drop", "keep", "maybe", "keep",
    ];
    let text = word_lines(&words);
    let query = json!({"path":"source.txt",
        "classification":{"predicates":["Is it kept?"]}});
    let unbudgeted = budgeted_read(&text, &query, 64);
    let mut result = budgeted_read(&text, &query, 3);
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(receipt.reason.as_deref(), Some("candidateBudget"));
    assert!(
        !result
            .warnings
            .iter()
            .any(|w| w.contains("were not judged")),
        "the receipt states the budget once: {:?}",
        result.warnings
    );
    let (mut totals, mut selected) = (Vec::new(), String::new());
    loop {
        assert_eq!(result.error_code, None, "{result:?}");
        let receipt = result.selection.as_ref().expect("receipt");
        totals.push(receipt.total_items);
        selected.push_str(result.content.as_deref().unwrap_or_default());
        let next = result.next.clone().expect("leads");
        let Some(page) = next.next_candidate_page else {
            break;
        };
        assert!(
            local_fetch_query::has_classification(&page.query)
                && local_fetch_query::selection(&page.query).is_some()
        );
        assert!(totals.len() < 5, "the chain ends");
        result = budgeted_read(&text, &serde_json::to_value(&page.query).unwrap(), 3);
    }
    assert_eq!(totals, [8, 8, 8], "each page counts the whole walk");
    let kept = |content: &str| {
        let mut lines = content
            .lines()
            .filter(|line| !line.starts_with("..."))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        lines.sort();
        lines
    };
    assert_eq!(
        kept(&selected),
        kept(unbudgeted.content.as_deref().unwrap()),
        "every selected chunk once"
    );
}

/// A candidate page replays the read of the source version its query pins:
/// the page keeps that snapshot and drops only private progress.
#[test]
fn a_candidate_page_keeps_the_snapshot_its_query_pins() {
    let words = ["keep", "drop", "maybe", "keep", "drop", "keep"];
    let text = word_lines(&words);
    let query = json!({"path":"source.txt",
        "classification":{"predicates":["Is it kept?"]}});
    let first = budgeted_read(&text, &query, 3);
    let pinned = first
        .source_sha256
        .clone()
        .expect("captured source identity");
    let mut pinned_query = query.clone();
    pinned_query["snapshot"] = json!(pinned);
    let result = budgeted_read(&text, &pinned_query, 3);
    assert_eq!(result.error_code, None, "{result:?}");
    let page = result
        .next
        .expect("leads")
        .next_candidate_page
        .expect("a page past the budget");
    assert_eq!(
        local_fetch_query::snapshot(&page.query),
        Some(pinned.as_str())
    );
}

/// However many unknown chunks a page leaves, `unfilteredUnknown` is one
/// replay that lists every one of them, none dropped past a row's `ranges`.
#[test]
fn one_replay_lists_every_unknown_chunk() {
    let words = (0..30)
        .map(|i| if i % 2 == 0 { "maybe" } else { "keep" })
        .collect::<Vec<_>>();
    let text = word_lines(&words);
    let query = json!({"path":"source.txt",
        "classification":{"predicates":["Is it kept?"]}});
    let result = budgeted_read(&text, &query, 64);
    let next = result.next.expect("leads");
    let replay = serde_json::to_value(&next.unfiltered_unknown.expect("replay").query).unwrap();
    let mut replayed = budgeted_read(&text, &replay, 64);
    let (mut maybe, mut keep) = (0, 0);
    let mut pages = 0;
    loop {
        assert_eq!(replayed.error_code, None, "{replayed:?}");
        let shown = replayed.content.as_deref().unwrap_or_default();
        maybe += shown.matches("maybe").count();
        keep += shown.matches("keep").count();
        pages += 1;
        let Some(page) = replayed
            .next
            .as_ref()
            .and_then(|next| next.continue_window.as_ref())
        else {
            break;
        };
        assert!(pages < 10, "the pages end");
        let page = serde_json::to_value(&page.query).unwrap();
        replayed = budgeted_read(&text, &page, 64);
    }
    assert_eq!((maybe, keep), (15, 0), "{pages} pages");
    assert!(
        replayed.selection.is_none(),
        "an undecided view has no receipt"
    );
    assert!(
        replayed
            .next
            .is_none_or(|next| next.unfiltered_unknown.is_none())
    );
}
