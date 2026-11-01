//! Admission and bounds: cancellation usage, reader and path policy, memory mode, aggregate bytes and state size.

use super::{Answer, png, provider, requests, row, runtime};
use crate::runtime_clasify::clasify;
use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, Request, Respond, ResponseTemplate};

#[derive(Clone)]
struct DelayedAnswer;
impl Respond for DelayedAnswer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        Answer
            .respond(request)
            .set_delay(std::time::Duration::from_secs(10))
    }
}

#[tokio::test]
async fn cancellation_keeps_completed_sibling_usage() {
    let server = provider().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_partial_json(
            json!({"state":"slow"}),
        ))
        .respond_with(DelayedAnswer)
        .with_priority(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = std::sync::Arc::new(runtime(
        &workspace,
        &server,
        &[("OCTOCODE_ENABLE_STATS", "true".into())],
    ));
    let fast = json!({"operation":"evaluate","content":"fast","questions":{"fact":{"type":"noul","instructions":"Is this supplied evidence?"}}});
    let mut slow = fast.clone();
    slow["content"] = json!("slow");
    let running = runtime.clone();
    let batch = fast.clone();
    let task = tokio::spawn(async move {
        running
            .run(
                "cancelled".into(),
                "clasify".into(),
                json!({"queries":[batch,slow]}),
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while requests(&server).await.len() < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // A replay joins the successful sibling's single flight. Its completion
    // proves the original judgment is saved before cancellation, without timing guesses.
    let replay = clasify(&runtime, vec![fast]).await;
    assert_eq!(replay["results"][0]["data"]["replayed"], true, "{replay}");
    assert!(runtime.requests.cancel("cancelled"));
    let _ = task.await.unwrap();
    let stats: Value =
        serde_json::from_slice(&std::fs::read(workspace.home.join("stats.json")).unwrap()).unwrap();
    assert_eq!(stats["stats"]["clasify"]["calls"], 2, "{stats}");
    assert_eq!(stats["stats"]["clasify"]["known_usage_calls"], 1, "{stats}");
    assert_eq!(
        stats["stats"]["clasify"]["unknown_usage_calls"], 1,
        "{stats}"
    );
    assert_eq!(stats["stats"]["clasify"]["input_tokens"], 100, "{stats}");
    assert_eq!(stats["stats"]["clasify"]["output_tokens"], 10, "{stats}");
    runtime.close().await;
}

#[tokio::test]
async fn image_asset_admission_respects_disabled_reader_and_path_policy() {
    let server = provider().await;
    let workspace = Workspace::new();
    let disabled = runtime(
        &workspace,
        &server,
        &[("DISABLE_TOOLS", "localFetch".into())],
    );
    let error = disabled
        .run_mcp(
            "disabled-media".into(),
            "clasify".into(),
            json!({"queries":[row("missing.png")]}),
        )
        .await
        .expect("MCP refusal response");
    assert_eq!(error["structuredContent"]["errorCode"], "invalidInput");
    assert!(
        error["structuredContent"]["error"]
            .as_str()
            .is_some_and(
                |message| message.contains("queries.0.assets.image_a.source")
                    && message.contains("localFetch")
                    && message.contains("DISABLE_TOOLS")
            ),
        "{error}"
    );
    assert!(requests(&server).await.is_empty());
    disabled.close().await;
    let enabled = runtime(&workspace, &server, &[]);
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("private.png");
    std::fs::write(&file, png()).unwrap();
    let out = clasify(&enabled, vec![row(file.to_str().unwrap())]).await;
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "securityValidationFailed",
        "{out}"
    );
    assert!(requests(&server).await.is_empty());
    enabled.close().await;
}

#[tokio::test]
async fn memory_mode_refuses_images_without_persistent_evidence() {
    let server = provider().await;
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("one.png"), png()).unwrap();
    let runtime = runtime(
        &workspace,
        &server,
        &[("OCTOCODE_STORAGE_MODE", "memory".into())],
    );
    let out = clasify(&runtime, vec![row("one.png")]).await;
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "classificationContextUnavailable",
        "{out}"
    );
    assert!(requests(&server).await.is_empty());
    assert!(!workspace.home.join("tmp/capture").exists());
    runtime.close().await;
}

#[tokio::test]
async fn aggregate_bytes_refuse_the_whole_image_row() {
    let server = provider().await;
    let workspace = Workspace::new();
    let mut bytes = png();
    bytes.resize(6 * 1024 * 1024, 0);
    std::fs::write(workspace.workspace.join("one.png"), bytes).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let mut query = row("one.png");
    query["content"] = json!(["image_a", "image_b", "image_c"]);
    query["assets"]["image_b"] = query["assets"]["image_a"].clone();
    query["assets"]["image_c"] = query["assets"]["image_a"].clone();
    let out = clasify(&runtime, vec![query]).await;
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "classificationContextTooLarge",
        "{out}"
    );
    assert!(requests(&server).await.is_empty());
    runtime.close().await;
}

#[tokio::test]
async fn text_source_and_image_share_one_judgment_without_host_body() {
    let server = provider().await;
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("one.png"), png()).unwrap();
    let body = "UNREAD_EVIDENCE_ONLY_5826: image_a documents the error screen.\n";
    std::fs::write(workspace.workspace.join("guide.md"), body).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let mut query = row("one.png");
    query.as_object_mut().unwrap().remove("content");
    query["source"] = json!({"tool":"localFetch","query":{"path":"guide.md"}});
    let out = clasify(&runtime, vec![query]).await;
    assert_eq!(
        out["results"][0]["data"]["answers"]["visible"]["noul"], 0.9,
        "{out}"
    );
    assert!(!out.to_string().contains("UNREAD_EVIDENCE_ONLY_5826"));
    let sent = requests(&server).await;
    assert_eq!(sent.len(), 1);
    let request: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert!(
        request["state"]
            .to_string()
            .contains("UNREAD_EVIDENCE_ONLY_5826")
    );
    assert_eq!(request["assets"].as_object().unwrap().len(), 1);
    runtime.close().await;
}

#[tokio::test]
async fn unicode_state_near_character_boundary_is_not_counted_as_utf8_bytes() {
    let server = provider().await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server, &[]);
    let text = "é".repeat(31_998);
    let out = clasify(&runtime, vec![json!({"operation":"evaluate","content":text,"questions":{"q":{"type":"noul","instructions":"Does the state contain accented letters?"}}})]).await;
    assert_eq!(
        out["results"][0]["data"]["answers"]["q"]["noul"], 0.9,
        "{out}"
    );
    let sent = requests(&server).await;
    assert_eq!(sent.len(), 1);
    let request: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(request["state"].as_str().unwrap().chars().count(), 31_998);
    runtime.close().await;
}

#[tokio::test]
async fn captured_state_includes_json_escaping_and_identity_in_its_bound() {
    let server = provider().await;
    let workspace = Workspace::new();
    // Source text alone fits 32k, but its JSON representation does not.
    std::fs::write(workspace.workspace.join("quoted.txt"), "\"".repeat(20_000)).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let out = clasify(&runtime, vec![json!({"operation":"evaluate","longContext":false,"source":{"tool":"localFetch","query":{"path":"quoted.txt"}},
        "questions":{"q":{"type":"noul","instructions":"Does the source contain quotation marks?"}}})]).await;
    assert_eq!(
        out["results"][0]["data"]["errorCode"], "classificationContextTooLarge",
        "{out}"
    );
    assert!(requests(&server).await.is_empty());
    runtime.close().await;
}
