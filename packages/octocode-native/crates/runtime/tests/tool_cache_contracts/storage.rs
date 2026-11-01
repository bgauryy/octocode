use super::*;

fn outline_entries(workspace: &Workspace) -> usize {
    std::fs::read_dir(workspace.home.join("tmp/outline"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "entry"))
        .count()
}

#[tokio::test]
async fn symbols_storage_is_owned_by_each_runtime_in_every_mode_order() {
    for (first_mode, second_mode) in [
        ("persistent", "memory"),
        ("memory", "persistent"),
        ("persistent", "persistent"),
    ] {
        let first = Workspace::new();
        let second = Workspace::new();
        for (workspace, mode, name) in [
            (&first, first_mode, "first"),
            (&second, second_mode, "second"),
        ] {
            let target =
                workspace.write("source.rs", format!("pub fn {name}() -> usize {{ 1 }}\n"));
            let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", mode.into())]);
            let outcome = call(
                &runtime,
                "localFetch",
                json!({"path": target, "view": "symbols"}),
            )
            .await
            .unwrap();
            assert_eq!(row_status(&outcome), "success");
            assert!(
                row_data(&outcome)["content"]
                    .as_str()
                    .unwrap()
                    .contains(name)
            );
            runtime.close().await;
            assert_eq!(
                outline_entries(workspace),
                usize::from(mode == "persistent"),
                "{first_mode} -> {second_mode}: {name} runtime must honor its own policy and home"
            );
        }
        assert_eq!(
            outline_entries(&first),
            usize::from(first_mode == "persistent")
        );
    }
}

#[tokio::test]
async fn graph_memory_storage_publishes_requested_snapshot_without_extraction_cache() {
    let workspace = Workspace::new();
    workspace.write("main.ts", "export function answer() { return 42; }\n");
    let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "memory".into())]);
    let outcome = call(&runtime, "graphBuild", json!({"path":workspace.workspace}))
        .await
        .unwrap();
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let graph = workspace.workspace.join(".octocode/graph");
    assert!(
        graph.join("latest").exists(),
        "the requested graph snapshot is an output"
    );
    assert!(
        !graph.join(".facts").exists(),
        "memory mode must not save reusable extraction artifacts"
    );
    runtime.close().await;
}

#[tokio::test]
async fn clone_receipts_are_complete_without_saved_response_cache() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"message":"Forbidden"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut settings = support::github_api(&server).to_vec();
    settings.push(("OCTOCODE_ENABLE_CLONE", "true".into()));
    let runtime = workspace.runtime(&settings);
    let outcome = call(&runtime, "ghCloneRepo", json!({"queries":[{"owner":"a","repo":"one"},{"owner":"b","repo":"two"}],"responseChars":1})).await.unwrap();
    assert_eq!(
        outcome.structured_content["results"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "clone receipts are never saved for later replay"
    );
    assert!(
        outcome
            .structured_content
            .pointer("/responsePagination/next")
            .is_none()
    );
    for dir in ["response", "response-pages", "continuations", "outline"] {
        assert!(
            !workspace.home.join("tmp").join(dir).exists(),
            "clone must not write {dir}"
        );
    }
    runtime.close().await;
}
