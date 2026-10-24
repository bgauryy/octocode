//! Provider failures and refused input.
use super::*;
use octocode_contract_types::LocalFetchQuery;

#[test]
fn a_first_classified_call_with_an_offset_is_refused_input_not_a_stale_cursor() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::source_view::SourceFacts;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"source.txt", "readProgress":{"unit":"lines","offset":3},
        "classification":{"predicates":["Is it kept?"]}
    }))
    .unwrap();
    let result = crate::tools::source_view::process_fetched_content_selected(
        &query,
        "a\nb\nc\nd\ne\nf\n".as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    );
    assert_eq!(
        result.error_code,
        Some(ErrorCode::InvalidInput),
        "{result:?}"
    );
    let recovery = result
        .next
        .expect("recovery")
        .unfiltered
        .expect("unfiltered")
        .query;
    // Recovery preserves the selection and restarts its unfiltered view.
    assert!(
        !local_fetch_query::has_classification(&recovery),
        "{recovery:?}"
    );
    assert!(local_fetch_query::read_progress(&recovery).is_none());
    assert_eq!(
        local_fetch_query::line_ranges(&recovery),
        local_fetch_query::line_ranges(&query)
    );
}

#[test]
fn a_provider_failing_every_chunk_fails_the_read_with_its_typed_error() {
    let result = word_read(REFUSED);
    assert_eq!(
        result.error_code,
        Some(ErrorCode::ClassificationProviderError)
    );
    assert!(
        result
            .hints
            .iter()
            .any(|hint| hint.contains("OCTOCODE_CLASSIFICATION_API"))
    );
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Unavailable
    );
    assert_eq!(
        receipt.reason.as_deref(),
        Some("classificationProviderError")
    );
    assert_eq!(receipt.unknown_items, 5);
    assert!(result.content.is_none(), "no unjudged body as a fallback");
    let recovery = result
        .next
        .expect("recovery")
        .unfiltered
        .expect("unfiltered")
        .query;
    assert!(!local_fetch_query::has_classification(&recovery));
}

/// A provider that cannot answer (down, overloaded, unreachable) never
/// withholds a read that succeeded: the whole view is shown under an
/// `unavailable` receipt naming the provider's code, the warning states
/// its error and hint, no unfiltered lead repeats the view, and a later
/// page replays that plan without another provider call.
#[test]
fn a_provider_that_is_down_shows_the_unfiltered_view_as_unavailable() {
    let result = word_read(DOWN);
    assert!(result.error_code.is_none(), "{result:?}");
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Unavailable
    );
    assert_eq!(
        receipt.reason.as_deref(),
        DOWN.map(crate::id::error_codes::as_str)
    );
    assert_eq!((receipt.total_items, receipt.unknown_items), (5, 5));
    let content = result.content.as_deref().expect("the view is shown");
    for word in ["keep", "drop", "maybe"] {
        assert!(content.contains(word), "{word} is shown: {content}");
    }
    assert_eq!(content.lines().count(), 5, "every chunk, no marker");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("HTTP 503")
                && warning.contains("shown unfiltered")
                && warning.contains("OCTOCODE_CLASSIFICATION_API")),
        "{:?}",
        result.warnings
    );
    let next = result.next.as_ref();
    assert!(
        next.is_none_or(|next| next.unfiltered.is_none() && next.next_candidate_page.is_none()),
        "the view is the unfiltered read: {next:?}"
    );
    // Paged one chunk at a time, the recorded plan replays on the next
    // page: the provider (now answering) is not asked, so the view stays
    // unavailable and whole.
    let facts = crate::tools::source_view::SourceFacts {
        window: Some(2_048),
        ..crate::tools::source_view::SourceFacts::default()
    };
    let first = word_read_with(json!({}), DOWN, &facts);
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("a next page")
        .query;
    assert!(
        local_fetch_query::selection(&next).is_some(),
        "the plan rides the next page"
    );
    let replay = word_read_with(
        serde_json::to_value(&next).expect("next query"),
        None,
        &facts,
    );
    assert_eq!(
        replay.selection.as_ref().map(|receipt| receipt.status),
        Some(octocode_contract_types::SelectionStatus::Unavailable),
        "replayed read: {replay:?}"
    );
    assert!(
        replay.error_code.is_none() && replay.content.as_deref().is_some_and(|t| !t.is_empty()),
        "the next page is shown: {replay:?}"
    );
}

/// A classified read that selects nothing shows no text, so it states no
/// line span or hit spans of the hidden view: the unfiltered leads carry
/// the coordinates that recover it.
#[test]
fn a_classified_read_that_selects_nothing_states_no_anchors() {
    let text = (0..40)
        .map(|n| {
            if n % 10 == 5 {
                format!("drop target {n}\n")
            } else {
                format!("maybe filler {n}\n")
            }
        })
        .collect::<String>();
    let query = json!({"path":"source.txt", "matchString":["target"], "contextLines":{"before":1,"after":1},
        "classification":{"predicates":["Is it kept?"]}});
    let result = budgeted_read(&text, &query, 64);
    assert_eq!(result.content.as_deref(), Some(""), "{result:?}");
    assert_eq!(
        result.selection.as_ref().expect("receipt").selected_items,
        0
    );
    let wire = serde_json::to_value(&result).unwrap();
    for key in ["startLine", "endLine", "matchRanges", "sourceLineRanges"] {
        assert!(wire.get(key).is_none(), "{key}: {wire}");
    }
    assert!(wire.pointer("/next/unfiltered").is_some(), "{wire}");
    // Zero kept is not absence: an empty row, recovered by its unfiltered lead.
    assert_eq!(result.status, ReadStatus::Empty);
}
