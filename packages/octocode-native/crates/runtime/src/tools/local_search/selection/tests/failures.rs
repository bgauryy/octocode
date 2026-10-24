//! Selections that keep nothing, or whose provider is down or refused.

use super::*;

/// A filter that keeps no hit leaves an empty row, not a success: the
/// unfiltered lead is its recovery, since zero kept is not absence.
#[test]
fn a_filter_that_keeps_no_hit_is_an_empty_row_with_its_unfiltered_lead() {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(root.path().join("a.rs"), "needle drop\nx\nneedle drop\n").expect("a");
    let result = search(root.path(), json!({}), None).expect("search");
    assert_eq!(result.selection.as_ref().expect("receipt").total_items, 2);
    assert!(result.files.is_empty());
    assert_eq!(result.status, SearchStatus::Empty);
    assert!(result.next.as_ref().unwrap().get("unfiltered").is_some());
}

#[test]
fn a_provider_that_is_down_shows_the_unfiltered_page_as_unavailable() {
    let root = fixture();
    let result = search(root.path(), json!({}), DOWN).expect("the search succeeded");
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
    let shown = result
        .files
        .iter()
        .map(|file| {
            (
                file.path.as_str(),
                file.matches.as_ref().map_or(0, Vec::len),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(shown, [("a.rs", 3), ("b.rs", 1), ("c.rs", 1)], "every hit");
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
        next.is_none_or(|next| next.get("unfiltered").is_none()),
        "the page is the unfiltered view: {next:?}"
    );
}

/// A refused credential is the caller's to fix: the row fails with the
/// provider's typed error and hint, never masked as a page, and its
/// unfiltered lead keeps the evidence one call away.
#[test]
fn a_refused_credential_fails_the_row_with_its_typed_error() {
    let root = fixture();
    let error = search(root.path(), json!({}), REFUSED).expect_err("refused credential");
    assert_eq!(error.code, ErrorCode::ClassificationProviderError);
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint.contains("OCTOCODE_CLASSIFICATION_API"))
    );
    let receipt = error.selection.as_ref().expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Unavailable
    );
    assert_eq!(
        receipt.reason.as_deref(),
        REFUSED.map(crate::id::error_codes::as_str)
    );
    assert_eq!(receipt.unknown_items, 5);
    let lead = &error.calls.as_ref().expect("evidence stays reachable")["unfiltered"];
    assert!(lead["input"]["queries"][0].get("classification").is_none());
}
