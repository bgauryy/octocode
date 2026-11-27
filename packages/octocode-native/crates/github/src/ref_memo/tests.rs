use super::*;
use crate::ProviderErrorKind;

/// An error a batch recorded for a ref names the repository as the row
/// that reads it spells it: another spelling resolves for itself.
#[tokio::test]
async fn a_recorded_ref_error_serves_only_the_spelling_it_named() {
    let memo = RefMemo::new();
    let error = ProviderError::new(ProviderErrorKind::NotFound, "Repository A/B not found");
    memo.record_ref("A", "B", None, Err(error));
    memo.refs_known();
    assert!(memo.resolved_ref("A", "B", None).await.is_some());
    assert!(memo.resolved_ref("a", "b", None).await.is_none());
}

/// A reader waits until the refs are known, and a memo settled without
/// an answer leaves the reader to resolve for itself.
#[tokio::test]
async fn a_reader_waits_for_the_refs_and_a_settled_memo_without_one_answers_none() {
    let memo = std::sync::Arc::new(RefMemo::new());
    let reader = tokio::spawn({
        let memo = memo.clone();
        async move { memo.resolved_ref("a", "b", Some("main")).await }
    });
    tokio::task::yield_now().await;
    assert!(!reader.is_finished(), "the reader waits for the refs");
    drop(SettleOnDrop(memo));
    assert!(reader.await.expect("reader").is_none());
}
