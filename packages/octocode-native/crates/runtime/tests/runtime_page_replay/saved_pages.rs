//! Saved pages: the input scope a snapshot pins, expiry, failed retention, and the read past the last page.

use super::lines;
use crate::support::{self, Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

async fn saved_fixture(
    workspace: &Workspace,
    runtime: &octocode_native::runtime::ToolRuntime,
) -> (Value, Value) {
    let file = workspace.write("saved-partitions.txt", lines("captured"));
    let query = json!({"path":file,"matchString":"needle","pageSize":50});
    let first = runtime
        .run(
            "save".into(),
            "localSearch".into(),
            json!({"queries":[query],"responseLength":800,"responseScope":"rows"}),
        )
        .await
        .expect("save");
    let next = first.structured_content["responsePagination"]["next"]["query"].clone();
    assert!(next.is_object(), "{}", first.structured_content);
    assert!(
        next["responseSnapshot"]
            .as_str()
            .is_some_and(|snapshot| snapshot.len() <= 128),
        "saved continuation must contain a snapshot string of at most 128 bytes: {next}"
    );
    octocode_native::contracts::validate("localSearch", next.clone())
        .expect("canonical continuation");
    (query, next)
}

#[tokio::test]
async fn saved_pages_retain_canonical_input_scope_across_persistent_replay() {
    for (scope, output_scope) in [("structured", "structuredContent"), ("rows", "rows")] {
        let workspace = Workspace::new();
        let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
        let runtime = workspace.runtime(&env);
        let files = (0..3)
            .map(|index| workspace.write(&format!("page-{index}.txt"), lines("captured")))
            .collect::<Vec<_>>();
        let queries = files
            .iter()
            .map(|file| json!({"path":file,"fullContent":true}))
            .collect::<Vec<_>>();
        let first = runtime
            .run(
                "capture".into(),
                "localFetch".into(),
                json!({"queries":queries,"responseLength":96,"responseScope":scope}),
            )
            .await
            .expect("initial page");
        assert_eq!(
            first.structured_content["responsePagination"]["scope"],
            output_scope
        );
        let next = first.structured_content["responsePagination"]["next"]["query"].clone();
        assert!(next.is_object(), "{}", first.structured_content);
        octocode_native::contracts::validate("localFetch", next.clone())
            .expect("canonical continuation");
        runtime.close().await;
        for file in files {
            std::fs::remove_file(file).expect("remove live source");
        }
        let runtime = workspace.runtime(&env);
        for omit_scope in [false, true] {
            let mut request = next.clone();
            if !omit_scope {
                // A scope sent beside the cursor must match it.
                request["responseScope"] = json!(scope);
            }
            let page = runtime
                .run("replay".into(), "localFetch".into(), request)
                .await
                .expect("saved page uses canonical scope");
            let pagination = &page.structured_content["responsePagination"];
            assert_eq!(pagination["scope"], output_scope);
            assert_eq!(pagination["replayed"], true);
            if let Some(next) = pagination["next"].get("query") {
                support::assert_cursor_only(next);
                octocode_native::contracts::validate("localFetch", next.clone())
                    .expect("canonical saved continuation");
            }
        }
        runtime.close().await;
    }
}

#[tokio::test]
async fn saved_pages_reject_mixed_input_tool_credential_config_and_workspace_changes() {
    use octocode_native::runtime::{Channel, ToolRuntime};
    let workspace = Workspace::new();
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let (query, next) = saved_fixture(&workspace, &runtime).await;
    for changed in [
        query.clone(),
        json!({"path":"/changed","matchString":"new question"}),
    ] {
        let mut mixed = next.clone();
        mixed["queries"] = json!([changed]);
        let error = runtime
            .run("mixed".into(), "localSearch".into(), mixed)
            .await
            .expect_err("mixed input");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    let error = runtime
        .run("tool".into(), "localFetch".into(), next.clone())
        .await
        .expect_err("wrong tool");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    let admission = runtime
        .admit_with(
            "credential".into(),
            Some(json!({"githubToken":"different-request-token"})),
        )
        .expect("admit");
    let error = runtime
        .execute(admission, "localSearch".into(), next.clone(), Channel::Cli)
        .await
        .expect_err("credential boundary");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    let admission = runtime.admit("surface".into()).expect("admit");
    let error = runtime
        .execute(admission, "localSearch".into(), next.clone(), Channel::Mcp)
        .await
        .expect_err("surface boundary");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    for changed in ["cwd", "config"] {
        let mut input = workspace.config(&env);
        if changed == "cwd" {
            input.cwd = workspace.workspace.join("subdirectory");
            std::fs::create_dir_all(&input.cwd).expect("cwd");
        } else {
            input
                .env
                .insert("OCTOCODE_ENABLE_STATS".into(), "true".into());
        }
        let other = ToolRuntime::new(input).expect("other runtime");
        let error = other
            .run(changed.into(), "localSearch".into(), next.clone())
            .await
            .expect_err("partition");
        assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
        other.close().await;
    }
    runtime.close().await;
}

#[tokio::test]
async fn expired_disk_snapshot_does_not_reexecute() {
    let workspace = Workspace::new();
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let (_, next) = saved_fixture(&workspace, &runtime).await;
    runtime.close().await;
    let dir = workspace.home.join("tmp/response-pages");
    // An entry file is its header line, then the value.
    for file in std::fs::read_dir(dir).expect("snapshot store").flatten() {
        if file.path().extension().is_some_and(|ext| ext == "entry") {
            let bytes = std::fs::read(file.path()).expect("entry");
            let end = bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .expect("header");
            let mut header: Value = serde_json::from_slice(&bytes[..end]).expect("header json");
            header["storedAtUnix"] = json!(1);
            let mut aged = header.to_string().into_bytes();
            aged.extend_from_slice(&bytes[end..]);
            std::fs::write(file.path(), aged).expect("age entry");
        }
    }
    let runtime = workspace.runtime(&env);
    let error = runtime
        .run("expired".into(), "localSearch".into(), next)
        .await
        .expect_err("expired snapshot");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    runtime.close().await;
}

#[tokio::test]
async fn failed_snapshot_retention_returns_complete_result_without_display_next() {
    let workspace = Workspace::new();
    std::fs::create_dir_all(workspace.home.join("tmp")).expect("cache parent");
    std::fs::write(workspace.home.join("tmp/response-pages"), "block directory")
        .expect("blocked cache");
    let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".into())]);
    let file = workspace.write("saved.txt", lines("all-evidence"));
    let response = runtime
        .run(
            "fallback".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":file,"matchString":"needle","pageSize":50}],
                "responseLength":800,"responseScope":"rows"
            }),
        )
        .await
        .expect("complete fallback");
    let output = response.structured_content;
    assert!(output.get("responsePagination").is_none(), "{output}");
    assert!(
        output["warnings"].to_string().contains("complete result"),
        "{output}"
    );
    assert_eq!(
        output["results"][0]["data"]["files"][0]["matches"]
            .as_array()
            .expect("complete match array")
            .len(),
        200
    );
    runtime.close().await;
}

/// A saved read past the last page of an unchanged response is a restart
/// page that ends the walk: `hasMore` is false, so a caller following `next`
/// while `hasMore` stops, and its `next` still starts over at offset 0. A
/// changed page size is refused as a caller error (exit class invalidInput).
#[tokio::test]
async fn a_saved_read_past_the_last_page_ends_the_walk() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let (_, next) = saved_fixture(&workspace, &runtime).await;
    let past = support::cursor_at_offset(&json!({"next": {"query": next}}), 9_999);
    let page = runtime
        .run("past".into(), "localSearch".into(), past)
        .await
        .expect("restart page");
    let pagination = &page.structured_content["responsePagination"];
    assert_eq!(pagination["restart"], true, "{pagination}");
    assert_eq!(pagination["hasMore"], false, "{pagination}");
    support::assert_cursor_only(&pagination["next"]["query"]);
    assert_eq!(support::cursor_offset(pagination), 0, "{pagination}");
    // Following the restart serves the first page.
    let first = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            pagination["next"]["query"].clone(),
        )
        .await
        .expect("first page again");
    let first = &first.structured_content["responsePagination"];
    assert_eq!(first["currentPage"], 1, "{first}");
    assert_ne!(first["restart"], true, "{first}");
    octocode_native::contracts::validate_output("localSearch", &page.structured_content)
        .expect("restart page output contract");
    let mut resized = next;
    resized["responseLength"] = json!(900);
    let error = runtime
        .run("resized".into(), "localSearch".into(), resized)
        .await
        .expect_err("a changed page size is refused");
    assert_eq!(error.code, ErrorCode::ResponsePageMismatch);
    assert_eq!(
        error.exit_class(),
        octocode_native::response::exit::ExitClass::InvalidInput
    );
    runtime.close().await;
}
