use super::*;

fn context(deadline: std::time::Duration) -> ExecutionContext {
    ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: Some(std::time::Instant::now() + deadline),
        response_window: None,
        github_credential: None,
    }
}

#[tokio::test]
async fn finished_work_returns_its_output() {
    let context = context(std::time::Duration::from_secs(60));
    assert_eq!(within_request(&context, None, async { 7 }).await, Ok(7));
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
        within_request(&context, None, std::future::pending::<()>()),
    )
    .await
    .expect("cancellation ends the work without waiting for it");
    assert_eq!(outcome, Err(ExecutionError::Cancelled));
}

/// A1: an lspSearch row on MCP whose caller set no budget still ends its
/// server waits before an MCP host's default 60 s request timeout, so the
/// host receives the `languageServerIndexing` row and its rerun lead; a
/// caller's own budget stands, and a channel with no default waits on.
#[test]
fn the_mcp_reply_budget_bounds_server_waits_only_without_a_caller_budget() {
    const HOST_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
    let budget = octocode_runtime_base::execution::RuntimeLimits::DEFAULT.mcp_reply_budget;
    assert!(budget < HOST_REQUEST_TIMEOUT);
    let now = std::time::Instant::now();
    assert_eq!(
        lsp_wait_deadline(None, Some(budget), now),
        Some(now + budget)
    );
    for caller in [
        now + std::time::Duration::from_secs(5),
        now + HOST_REQUEST_TIMEOUT * 10,
    ] {
        assert_eq!(
            lsp_wait_deadline(Some(caller), Some(budget), now),
            Some(caller)
        );
    }
    assert_eq!(lsp_wait_deadline(None, None, now), None);
}
