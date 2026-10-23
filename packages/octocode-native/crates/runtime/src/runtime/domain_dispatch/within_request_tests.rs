use super::*;

fn context(deadline: std::time::Duration) -> ExecutionContext {
    ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() + deadline,
        walk_threads: None,
        response_window: None,
        github_credential: None,
    }
}

#[tokio::test]
async fn finished_work_returns_its_output() {
    let context = context(std::time::Duration::from_secs(60));
    assert_eq!(within_request(&context, async { 7 }).await, Ok(7));
}

/// A cancelled request does not wait for its language-server work.
#[tokio::test]
async fn cancelling_the_request_drops_pending_work_at_once() {
    let context = context(std::time::Duration::from_secs(600));
    let token = context.cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        token.cancel();
    });
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        within_request(&context, std::future::pending::<()>()),
    )
    .await
    .expect("cancellation ends the work without waiting for it");
    assert_eq!(outcome, Err(ExecutionError::Cancelled));
}
