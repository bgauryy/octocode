//! A host that exits cannot promise a later pinned read from memory storage.
use super::{provider, provider_calls, separate_functions};
use crate::support::{Workspace, clasify_runtime};
use octocode_native::runtime::{Channel, ToolRuntime};
use serde_json::{Value, json};

fn questions() -> Value {
    json!({"q":{"type":"noul","instructions":"Does this say restart?"}})
}

fn source(path: &str) -> Value {
    json!({"tool":"localFetch","query":{"path":path}})
}

async fn exiting_call(runtime: &ToolRuntime, queries: Vec<Value>) -> Value {
    let admission = runtime
        .admit("exiting-clasify".into())
        .unwrap()
        .requiring_persistent_continuations();
    let outcome = runtime
        .execute(
            admission,
            "clasify".into(),
            json!({"queries":queries}),
            Channel::Cli,
        )
        .await
        .expect("row refusals remain valid tool responses");
    crate::support::checks::assert_output_contract("clasify", &outcome.structured_content);
    outcome.structured_content.into_value()
}

fn refused(row: &Value) {
    assert_eq!(row["status"], "error", "{row:#}");
    assert_eq!(
        row["data"]["errorCode"], "classificationContextUnavailable",
        "{row:#}"
    );
    assert!(
        row["data"]["error"]
            .as_str()
            .unwrap()
            .contains("no source was read"),
        "{row:#}"
    );
    assert!(
        row["data"]["hints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint.as_str().unwrap().contains("persistent storage")),
        "{row:#}"
    );
    assert!(
        row["data"].get("usage").is_none(),
        "no provider usage: {row:#}"
    );
    assert!(
        !row.to_string().contains("\"cursor\""),
        "no dead handles: {row:#}"
    );
}

#[tokio::test]
async fn exiting_memory_clasify_refuses_sources_before_capture_or_billing() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("simple.rs", "pub fn restart() {}\n");
    workspace.write("many.rs", separate_functions(70));
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[
            ("OCTOCODE_STORAGE_MODE", "memory".into()),
            ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ],
    );
    let sources = [
        source("simple.rs"),
        source("many.rs"),
        source("never-created.rs"),
        json!({"tool":"ghGetFileContent","query":{"owner":"octo","repo":"demo","path":"source.rs","ref":"main"}}),
    ];
    for source in sources {
        let rows = vec![
            json!({"operation":"sections","resources":{"source":source},"questions":questions()}),
            json!({"operation":"evaluate","source":source,"questions":questions()}),
        ];
        let output = exiting_call(&runtime, rows).await;
        let rows = output["results"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        rows.iter().for_each(refused);
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "neither source GETs nor provider POSTs"
    );
    runtime.close().await;
}

#[tokio::test]
async fn exiting_memory_clasify_keeps_literal_siblings_and_long_lived_source_reads() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("restart.rs", "pub fn restart() {}\n");
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[("OCTOCODE_STORAGE_MODE", "memory".into())],
    );
    let literal =
        json!({"operation":"evaluate","content":"Restart the service.","questions":questions()});
    let evaluation =
        json!({"operation":"evaluate","source":source("restart.rs"),"questions":questions()});
    let sections = json!({"operation":"sections","resources":{"source":source("restart.rs")},"questions":questions()});
    let output = exiting_call(&runtime, vec![sections, literal, evaluation.clone()]).await;
    refused(&output["results"][0]);
    refused(&output["results"][2]);
    assert_eq!(
        output["results"][1]["data"]["answers"]["q"]["noul"], 0.9,
        "{output:#}"
    );
    assert_eq!(
        provider_calls(&server).await,
        1,
        "only the literal sibling is judged"
    );

    // Request lifetime, not channel naming or storage mode alone, owns retention.
    for (index, channel) in [Channel::Cli, Channel::Mcp].into_iter().enumerate() {
        let outcome = runtime
            .execute(
                runtime.admit("living-clasify".into()).unwrap(),
                "clasify".into(),
                json!({"queries":[evaluation.clone()]}),
                channel,
            )
            .await
            .unwrap();
        crate::support::checks::assert_output_contract("clasify", &outcome.structured_content);
        let output = outcome.structured_content.into_value();
        assert_eq!(
            output["results"][0]["data"]["answers"]["q"]["noul"], 0.9,
            "{output:#}"
        );
        let next = &output["results"][0]["data"]["next"]["read"];
        assert_eq!(next["kind"], "continue", "{next:#}");
        assert!(next["input"]["cursor"].is_string(), "{next:#}");
        let read = runtime
            .execute(
                runtime.admit("read-capture".into()).unwrap(),
                "localFetch".into(),
                next["input"].clone(),
                channel,
            )
            .await
            .unwrap()
            .structured_content
            .into_value();
        assert!(read.to_string().contains("pub fn restart() {}"), "{read:#}");
        assert_eq!(
            provider_calls(&server).await,
            index + 2,
            "following the exact read never bills again"
        );
    }
    runtime.close().await;
}
