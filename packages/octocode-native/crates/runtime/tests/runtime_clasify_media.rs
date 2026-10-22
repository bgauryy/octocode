//! Original image bytes stay inside the runtime/provider boundary.
use crate::runtime_clasify::clasify;
use crate::support::{Run as _, Workspace, capture_allowed, clasify_runtime};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_contract_types::ErrorCode;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[derive(Clone)]
struct DelayedAnswer;
impl Respond for DelayedAnswer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        Answer
            .respond(request)
            .set_delay(std::time::Duration::from_secs(10))
    }
}

fn png() -> Vec<u8> {
    STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAARgAAAEYCAAAAACi5bZQAAABXElEQVR42u3QAQ0AAAwCIPuX1gJP8EEEUk5RIEaMGDFixIgRI0aMGMSIESNGjBgxYsSIEYMYMWLEiBEjRowYMWIQI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIEYMYMWLEiBEjRowYMWIQI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsQgRowYMWLEiBEjRowYxIgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsQgRowYMWLEiBEjRowYxIgRI0aMGDFixIgRgxgxYsSIESNGjBgxYsQoECNGjBgxYsSIESNGDGLEiBEjRowYMWK+G5X/H6AJTpTJAAAAAElFTkSuQmCC").unwrap()
}

#[derive(Clone)]
struct Answer;
impl Respond for Answer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let answers: serde_json::Map<String, Value> = body["questions"].as_object().unwrap().iter()
            .map(|(key, question)| (key.clone(), if question["type"] == "box" {
                json!({"type":"box","box":{"top":0.1,"bottom":0.9,"left":0.2,"right":0.8},"confidence":null})
            } else { json!({"type":"noul","noul":0.9}) })).collect();
        ResponseTemplate::new(200).set_body_json(json!({"model":"jev-preview","answers":answers,
            "usage":{"input_tokens":100,"output_tokens":10}}))
    }
}

async fn provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(Answer)
        .mount(&server)
        .await;
    server
}

/// A clasify runtime that may also read what URL capture saved.
fn runtime(workspace: &Workspace, server: &MockServer, extra: &[(&str, String)]) -> ToolRuntime {
    let mut env = vec![capture_allowed(workspace)];
    env.extend(extra.iter().cloned());
    clasify_runtime(workspace, server, &env)
}

fn row(path: &str) -> Value {
    json!({"state":"image_a","assets":{"image_a":{"type":"image","source":{"tool":"localFetch","query":{"path":path}}}},
        "questions":{"visible":{"type":"noul","instructions":"Does image_a contain visible content?"},
        "bounds":{"type":"box","instructions":"Locate the object in image_a."}}})
}

async fn requests(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap()
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
    let fast = json!({"state":"fast","questions":{"fact":{"type":"noul","instructions":"Is this supplied evidence?"}}});
    let mut slow = fast.clone();
    slow["state"] = json!("slow");
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
    assert_eq!(replay["queries"][0]["replayed"], true, "{replay}");
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
async fn unread_local_image_is_byte_exact_hidden_and_saved_with_box() {
    let server = provider().await;
    let workspace = Workspace::new();
    let bytes = png();
    std::fs::write(workspace.workspace.join("one.png"), &bytes).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let out = clasify(&runtime, vec![row("one.png")]).await;
    let result = &out["queries"][0];
    assert_eq!(result["answers"]["bounds"]["box"]["left"], 0.2, "{out}");
    let asset = &result["assets"]["image_a"];
    assert_eq!(
        (asset["width"].as_u64(), asset["height"].as_u64()),
        (Some(280), Some(280))
    );
    assert_eq!(asset["format"], "png");
    assert_eq!(
        std::fs::read(asset["path"].as_str().unwrap()).unwrap(),
        bytes
    );
    assert!(asset["snapshot"].as_str().unwrap().len() == 64);
    assert!(
        result.get("hints").is_none(),
        "images do not advertise localFetch of binary"
    );
    assert!(!out.to_string().contains("data_base64"));
    assert!(!out.to_string().contains(&STANDARD.encode(&bytes)));
    let sent = requests(&server).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].headers.get("typesafe-danger-zone-images").unwrap(),
        "true"
    );
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(
        body["assets"]["image_a"]["data_base64"],
        STANDARD.encode(&bytes)
    );
    assert_eq!(body["state"], "image_a");
    runtime.close().await;
}

#[tokio::test]
async fn image_mutation_invalidates_judgment_and_old_evidence_survives() {
    let server = provider().await;
    let workspace = Workspace::new();
    let path = workspace.workspace.join("one.png");
    std::fs::write(&path, png()).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let first = clasify(&runtime, vec![row("one.png")]).await;
    let repeated = clasify(&runtime, vec![row("one.png")]).await;
    assert_eq!(repeated["queries"][0]["replayed"], true);
    let old = &first["queries"][0]["assets"]["image_a"];
    let mut changed = png();
    changed.push(0);
    std::fs::write(&path, &changed).unwrap();
    let newer = clasify(&runtime, vec![row("one.png")]).await;
    assert_ne!(
        newer["queries"][0]["assets"]["image_a"]["snapshot"],
        old["snapshot"]
    );
    assert_eq!(std::fs::read(old["path"].as_str().unwrap()).unwrap(), png());
    let mut pinned = row("one.png");
    pinned["assets"]["image_a"]["source"]["query"]["snapshot"] = old["snapshot"].clone();
    let stale = clasify(&runtime, vec![pinned]).await;
    assert_eq!(
        stale["queries"][0]["error"]["errorCode"], "staleSnapshot",
        "{stale}"
    );
    assert_eq!(requests(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn question_only_placeholders_and_multiple_assets_are_preserved() {
    let server = provider().await;
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("one.png"), png()).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let mut query = row("one.png");
    query["state"] = json!({"purpose":"screen two independent images"});
    query["assets"]["image_b"] = query["assets"]["image_a"].clone();
    query["questions"]["visible"]["instructions"] =
        json!("Does image_a show the same shape as image_b?");
    let out = clasify(&runtime, vec![query]).await;
    assert_eq!(
        out["queries"][0]["assets"].as_object().unwrap().len(),
        2,
        "{out}"
    );
    let sent = requests(&server).await;
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(body["assets"].as_object().unwrap().len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn invalid_missing_oversized_and_private_images_never_reach_provider() {
    let server = provider().await;
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("bad.png"), b"not an image").unwrap();
    let mut large_dimensions = png();
    large_dimensions[16..20].copy_from_slice(&9000u32.to_be_bytes());
    std::fs::write(workspace.workspace.join("wide.png"), large_dimensions).unwrap();
    let oversized = std::fs::File::create(workspace.workspace.join("large.png")).unwrap();
    oversized.set_len(8 * 1024 * 1024 + 1).unwrap();
    let runtime = runtime(&workspace, &server, &[]);
    let mut unused = row("missing.png");
    unused["assets"]["unused"] = unused["assets"]["image_a"].take();
    unused["assets"].as_object_mut().unwrap().remove("image_a");
    let mut url = row("unused");
    url["assets"]["image_a"]["source"] = json!({"url":"https://127.0.0.1:9/image.png"});
    let out = clasify(
        &runtime,
        vec![
            row("bad.png"),
            row("wide.png"),
            row("large.png"),
            unused,
            url,
        ],
    )
    .await;
    let errors: Vec<_> = out["queries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["error"]["errorCode"].as_str().unwrap())
        .collect();
    assert_eq!(
        errors,
        [
            "invalidClassificationContext",
            "classificationContextTooLarge",
            "classificationContextTooLarge",
            "invalidClassificationContext",
            "securityValidationFailed"
        ],
        "{out}"
    );
    assert!(requests(&server).await.is_empty());
    runtime.close().await;
}

#[tokio::test]
async fn github_image_reuses_shared_capture_cache_and_resolved_commit() {
    let server = provider().await;
    let github = MockServer::builder().start().await;
    let sha = "1111111111111111111111111111111111111111";
    let other_sha = "2222222222222222222222222222222222222222";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":sha})))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/other"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":other_sha})))
        .mount(&github)
        .await;
    let mut other_image = png();
    other_image.push(0);
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/image.png")).and(query_param("ref", other_sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode(&other_image),"size":other_image.len(),"sha":other_sha,"path":"image.png"})))
        .expect(1).mount(&github).await;
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/image.png")).and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode(png()),"size":png().len(),"sha":sha,"path":"image.png"})))
        .expect(1).mount(&github).await;
    let workspace = Workspace::new();
    let runtime = runtime(
        &workspace,
        &server,
        &[("GITHUB_API_URL", format!("{}/api/v3", github.uri()))],
    );
    let mut query = row("unused");
    query["assets"]["image_a"]["source"] = json!({"tool":"ghGetFileContent","query":{"owner":"a","repo":"b","path":"image.png","ref":"main"}});
    let out = clasify(&runtime, vec![query.clone()]).await;
    assert_eq!(
        out["queries"][0]["assets"]["image_a"]["source"]["query"]["ref"], sha,
        "{out}"
    );
    let again = clasify(&runtime, vec![query.clone()]).await;
    assert_eq!(again["queries"][0]["replayed"], true);
    let text = runtime
        .run(
            "text".into(),
            "ghGetFileContent".into(),
            json!({"queries":[{
                "owner":"a","repo":"b","path":"image.png","ref":sha,"ranges":["1-5"]
            }]}),
        )
        .await
        .unwrap()
        .structured_content;
    assert_eq!(
        text["results"][0]["data"]["errorCode"], "invalidInput",
        "{text}"
    );
    assert!(!text.to_string().contains(&STANDARD.encode(png())));
    query["assets"]["image_a"]["source"]["query"]["ref"] = json!("other");
    let other = clasify(&runtime, vec![query]).await;
    assert_eq!(
        other["queries"][0]["assets"]["image_a"]["source"]["query"]["ref"], other_sha,
        "{other}"
    );
    assert_ne!(
        other["queries"][0]["assets"]["image_a"]["snapshot"],
        out["queries"][0]["assets"]["image_a"]["snapshot"]
    );
    assert_eq!(requests(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn models_and_long_context_follow_provider_contract() {
    let server = provider().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"models":[{"name":"jev-preview","description":"vision","release_date":"2026-09-10T18:39:06.057655+00:00"}]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![
            json!({"operation":"models"}),
            json!({"state":"x".repeat(40_000),"longContext":true,
        "questions":{"q":{"type":"noul","instructions":"Is text present?"}}}),
        ],
    )
    .await;
    assert_eq!(
        out["queries"][0]["models"][0]["name"], "jev-preview",
        "{out}"
    );
    assert_eq!(out["queries"][1]["answers"]["q"]["noul"], 0.9, "{out}");
    let sent = requests(&server).await;
    let posted = sent
        .iter()
        .find(|request| request.method == "POST")
        .unwrap();
    assert_eq!(
        posted
            .headers
            .get("typesafe-danger-zone-long-context")
            .unwrap(),
        "true"
    );
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
        .run(
            "disabled-media".into(),
            "clasify".into(),
            json!({"queries":[row("missing.png")]}),
        )
        .await
        .expect_err("disabled source reader must be rejected at admission");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(
        error
            .validation_issues
            .unwrap()
            .iter()
            .any(|issue| issue.path == ["queries", "0", "assets", "image_a", "source"])
    );
    disabled.close().await;
    let enabled = runtime(&workspace, &server, &[]);
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("private.png");
    std::fs::write(&file, png()).unwrap();
    let out = clasify(&enabled, vec![row(file.to_str().unwrap())]).await;
    assert_eq!(
        out["queries"][0]["error"]["errorCode"], "securityValidationFailed",
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
        out["queries"][0]["error"]["errorCode"], "classificationContextUnavailable",
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
    query["state"] = json!(["image_a", "image_b", "image_c"]);
    query["assets"]["image_b"] = query["assets"]["image_a"].clone();
    query["assets"]["image_c"] = query["assets"]["image_a"].clone();
    let out = clasify(&runtime, vec![query]).await;
    assert_eq!(
        out["queries"][0]["error"]["errorCode"], "classificationContextTooLarge",
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
    query.as_object_mut().unwrap().remove("state");
    query["source"] = json!({"tool":"localFetch","query":{"path":"guide.md","fullContent":true}});
    let out = clasify(&runtime, vec![query]).await;
    assert_eq!(
        out["queries"][0]["answers"]["visible"]["noul"], 0.9,
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
    let out = clasify(&runtime, vec![json!({"state":text,"questions":{"q":{"type":"noul","instructions":"Does the state contain accented letters?"}}})]).await;
    assert_eq!(out["queries"][0]["answers"]["q"]["noul"], 0.9, "{out}");
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
    let out = clasify(&runtime, vec![json!({"source":{"tool":"localFetch","query":{"path":"quoted.txt","fullContent":true}},
        "questions":{"q":{"type":"noul","instructions":"Does the source contain quotation marks?"}}})]).await;
    assert_eq!(
        out["queries"][0]["error"]["errorCode"], "classificationContextTooLarge",
        "{out}"
    );
    assert!(requests(&server).await.is_empty());
    runtime.close().await;
}
