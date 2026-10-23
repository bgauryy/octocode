use super::*;

#[test]
fn local_fetch_fills_the_rows_share_and_other_tools_the_automatic_page() {
    let context = |response_window| ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
        walk_threads: None,
        response_window,
        github_credential: None,
    };
    let fetch = |window| local_response_window(ToolId::LocalFetch, &context(window), 20_000);
    assert_eq!(fetch(Some(5_000)), Some(5_000));
    assert_eq!(fetch(Some(50_000)), Some(50_000));
    assert_eq!(fetch(None), Some(20_000));
    // Every other local tool, lspSearch included, pages under the one
    // automatic window whatever the batch's own `responseLength`.
    for tool in [
        ToolId::LocalSearch,
        ToolId::StructureSearch,
        ToolId::AstSearch,
        ToolId::LspSearch,
    ] {
        for window in [None, Some(5_000), Some(50_000)] {
            assert_eq!(
                local_response_window(tool, &context(window), 20_000),
                Some(20_000),
                "{tool} {window:?}"
            );
        }
    }
    assert_eq!(
        local_response_window(ToolId::LspSearch, &context(None), 0),
        None
    );
}
