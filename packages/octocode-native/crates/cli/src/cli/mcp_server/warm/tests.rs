use super::*;

/// Windows has no warm server, so no call is routed to one: MCP runs every
/// tool in process there.
#[tokio::test]
async fn an_mcp_session_on_windows_runs_every_tool_in_process() {
    let warm = Warm::resolve();
    let cancelled = tokio_util::sync::CancellationToken::new();
    assert!(
        warm.call("localSearch", &Value::Null, &cancelled)
            .await
            .is_none()
    );
}
