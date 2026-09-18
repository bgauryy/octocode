mod support;

use serde_json::json;
use support::{Workspace, call, query_path, row_data, row_status};

#[tokio::test]
async fn runtime_requires_explicit_non_blank_reasoning() {
    let workspace = Workspace::new();
    let path = workspace.write("reasoning.txt", "ok\n");
    let runtime = workspace.runtime(&[]);
    let path = path.to_string_lossy().into_owned();
    for query in [json!({"path":path}), json!({"path":path,"reasoning":"   "})] {
        let error = runtime
            .execute("reasoning-test".into(), "localFetch".into(), query)
            .await
            .expect_err("reasoning is required");
        assert_eq!(error.code, "invalidInput");
    }
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_pages_and_unions_through_the_runtime() {
    let workspace = Workspace::new();
    let path = workspace.write("source.txt", "one\ntwo 😀\nthree\n");
    let runtime = workspace.runtime(&[]);
    let first = call(
        &runtime,
        "localFetch",
        query_path(&path, json!({"chunkType":"lines","limit":1})),
    )
    .await
    .expect("first page");
    assert_eq!(row_status(&first), "success");
    let data = row_data(&first);
    assert_eq!(data["content"].as_str(), Some("one\n"));
    let next = data["next"]["continue"]["query"].clone();
    assert!(next.is_object(), "executable continuation");

    let second = call(&runtime, "localFetch", next)
        .await
        .expect("second page");
    assert_eq!(row_data(&second)["content"].as_str(), Some("two 😀\n"));
    runtime.close().await;
}

#[tokio::test]
async fn universal_cli_continuations_resume_through_normal_validation() {
    let workspace = Workspace::new();
    workspace.write("src/main.rs", "pub fn main_marker() {}\n");
    let runtime = workspace.runtime(&[]);
    let query = json!({"operation":"files","path":workspace.workspace,"limit":1});
    let continuation_call = json!({"tool":"astSearch","query":query});
    let token = runtime
        .continuation_token(&continuation_call, None)
        .expect("continuation token");
    let (tool, resumed, source_digest) = runtime.resume_token(&token).expect("resume token");
    assert_eq!(tool, "astSearch");
    assert_eq!(resumed, query);
    assert!(source_digest.is_none());

    let outcome = call(&runtime, &tool, resumed)
        .await
        .expect("validated resumed query");
    assert_eq!(row_status(&outcome), "success");
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_rejects_unknown_fields_at_the_contract() {
    let workspace = Workspace::new();
    let path = workspace.write("a.txt", "ok\n");
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "localFetch",
        json!({"path": path, "madeUp": true}),
    )
    .await
    .expect_err("unknown field");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}

#[tokio::test]
async fn local_search_finds_literal_matches() {
    let workspace = Workspace::new();
    workspace.write("src/main.ts", "export function needle() { return 1; }\n");
    workspace.write("src/other.ts", "const unused = 2;\n");
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({
            "path": workspace.workspace,
            "searchText": "needle",
            "regex": "literal"
        }),
    )
    .await
    .expect("search");
    assert_eq!(row_status(&found), "success");
    let rendered = serde_json::to_string(row_data(&found)).expect("json");
    assert!(
        rendered.contains("main.ts"),
        "expected main.ts in {rendered}"
    );

    runtime.close().await;
}

#[tokio::test]
async fn disabled_local_family_is_unavailable() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("ENABLE_LOCAL", "false".into())]);
    let error = call(
        &runtime,
        "localFetch",
        json!({"path": workspace.workspace.join("missing.txt")}),
    )
    .await
    .expect_err("disabled");
    assert_eq!(error.code, "toolUnavailable");
    runtime.close().await;
}

#[tokio::test]
async fn runtime_catalog_lists_available_tools() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let catalog = runtime.catalog().expect("catalog");
    let names: Vec<_> = catalog["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"localFetch"));
    assert!(names.contains(&"localSearch"));
    assert!(names.contains(&"ghSearch"));
    runtime.close().await;
}

#[tokio::test]
async fn ast_search_lists_files_through_the_runtime() {
    let workspace = Workspace::new();
    workspace.write("src/lib.rs", "pub fn needle() {}\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "astSearch",
        json!({
            "operation": "files",
            "path": workspace.workspace
        }),
    )
    .await
    .expect("astSearch");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(rendered.contains("lib.rs"), "expected lib.rs in {rendered}");
    runtime.close().await;
}

#[tokio::test]
async fn lsp_search_returns_a_typed_row_when_no_server_is_configured() {
    let workspace = Workspace::new();
    let path = workspace.write("notes.md", "# title\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "uri": path
        }),
    )
    .await
    .expect("lspSearch");
    let status = row_status(&outcome);
    assert!(
        status == "error" || status == "empty" || status == "success",
        "unexpected status {status}: {}",
        outcome.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn lsp_search_rejects_files_outside_allowed_roots_before_server_discovery() {
    let workspace = Workspace::new();
    let outside = workspace.write_outside_allowed_roots("secret.rs", "pub fn secret() {}\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "uri": outside
        }),
    )
    .await
    .expect("typed lsp denial");
    assert_eq!(row_status(&outcome), "error");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("outside allowed directories"),
        "expected path-policy denial, got {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn resolved_lsp_config_path_reaches_engine_discovery() {
    let workspace = Workspace::new();
    let path = workspace.write("source.custom", "symbol\n");
    let config_path = workspace.write(
        ".octocode/lsp.json",
        r#"{"languageServers":{".custom":{"command":"missing-custom-lsp","args":[],"languageId":"custom"}}}"#,
    );
    let runtime = workspace.runtime(&[(
        "OCTOCODE_LSP_CONFIG",
        config_path.to_string_lossy().into_owned(),
    )]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "uri": path
        }),
    )
    .await
    .expect("typed configured server failure");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        !rendered.contains("No language server is configured"),
        "resolved config was ignored: {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn close_joins_a_fresh_runtime() {
    let workspace = Workspace::new();
    workspace.runtime(&[]).close().await;
}
