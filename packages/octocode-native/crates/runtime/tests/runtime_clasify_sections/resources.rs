//! How each resource kind is acquired: GitHub files at a pinned commit, URLs to internal addresses, and local ranges.

use super::{error_code, judged, provider, provider_calls, sections};
use crate::support::{Run as _, Workspace, clasify_runtime, github_api};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "1111111111111111111111111111111111111111";

/// A GitHub whose `a/b` repository serves each `(file, body)` at ref `v1`.
async fn github_with(files: &[(&str, &[u8])]) -> MockServer {
    let github = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&github)
        .await;
    for (file, body) in files {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/docs%2F{file}")))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body),
                "size":body.len(),"sha":"2".repeat(40),"path":format!("docs/{file}")
            })))
            .mount(&github)
            .await;
    }
    github
}

fn github_source(file: &str) -> Value {
    json!({"tool":"ghGetFileContent","query":{
        "owner":"a","repo":"b","path":format!("docs/{file}"),"ref":"v1"}})
}

#[tokio::test]
async fn a_github_resource_is_planned_from_its_pinned_commit() {
    let provider = provider().await;
    let github = github_with(&[("guide.md", b"# Guide\n\nRestart the service.\n")]).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &provider, &github_api(&github));
    let out = sections(&runtime, json!({"guide": github_source("guide.md")})).await;
    let guide = &out["resources"]["guide"];
    assert_eq!(guide["status"], "ready", "{out:#}");
    assert_eq!(guide["coordinateSpace"], "source");
    assert_eq!(
        guide["snapshot"], SHA,
        "the requested ref is pinned to its commit"
    );
    crate::support::assert_cursor_only(&guide["read"]["input"]);
    let read = runtime
        .run(
            "read-assessed-source".into(),
            "ghGetFileContent".into(),
            guide["read"]["input"].clone(),
        )
        .await
        .expect("source read");
    let requests = github.received_requests().await.expect("GitHub requests");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().contains("/commits/"))
            .count(),
        1,
        "the captured read never re-resolves the mutable ref"
    );
    for request in requests
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
    {
        assert!(
            request
                .url
                .query_pairs()
                .any(|(key, value)| key == "ref" && value == SHA)
        );
    }
    assert!(
        read.structured_content["results"][0]["data"]["content"]
            .as_str()
            .unwrap()
            .contains("Restart the service.")
    );
    assert_eq!(
        guide["sections"][0]["target"]["lines"],
        json!({"startLine":1,"endLine":3})
    );
    assert_eq!(guide["sections"][0]["answers"]["q"]["status"], "answered");
    assert!(judged(&provider).await.contains("Restart the service."));
    runtime.close().await;
}

#[tokio::test]
async fn a_github_resource_must_be_utf8_text_and_fails_alone() {
    let provider = provider().await;
    let github = github_with(&[
        ("ok.md", b"Restart the service.\n"),
        ("blob.md", b"MZ\0\0binary\0payload"),
        ("latin1.md", b"caf\xe9 au lait, please restart\n"),
    ])
    .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &provider, &github_api(&github));
    let out = sections(
        &runtime,
        json!({
            "ok": github_source("ok.md"),
            "blob": github_source("blob.md"),
            "latin1": github_source("latin1.md"),
        }),
    )
    .await;
    let resources = &out["resources"];
    assert_eq!(resources["ok"]["status"], "ready", "{out:#}");
    for (key, message) in [
        ("blob", "requires text; this resource is binary"),
        ("latin1", "not valid UTF-8 text"),
    ] {
        let resource = &resources[key];
        assert_eq!(resource["status"], "error", "{key}: {out:#}");
        assert_eq!(error_code(resource), "classificationContextUnavailable");
        assert!(
            resource["error"]["error"]
                .as_str()
                .is_some_and(|text| text.contains(message)),
            "{key}: {out:#}"
        );
        assert!(resource["sections"].is_null() || resource["sections"] == json!([]));
        assert_eq!(resource["read"]["tool"], "ghGetFileContent", "{key}");
    }
    let sent = judged(&provider).await;
    assert!(sent.contains("Restart the service."));
    assert!(
        !sent.contains("payload") && !sent.contains("au lait"),
        "{sent}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_url_resource_to_an_internal_address_fails_alone_before_any_request() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write("notes.txt", "Restart the service.\n");
    let runtime = clasify_runtime(
        &workspace,
        &provider,
        &[crate::support::capture_allowed(&workspace)],
    );
    let out = sections(
        &runtime,
        json!({
            "page": {"url": "https://127.0.0.1:9/guide"},
            "notes": {"tool":"localFetch","query":{"path":"notes.txt"}},
        }),
    )
    .await;
    let page = &out["resources"]["page"];
    assert_eq!(page["status"], "error", "{out:#}");
    assert_eq!(error_code(page), "securityValidationFailed");
    assert_eq!(out["resources"]["notes"]["status"], "ready", "{out:#}");
    assert_eq!(
        provider_calls(&provider).await,
        1,
        "only the readable resource was judged"
    );
    runtime.close().await;
}

#[tokio::test]
async fn local_ranges_bound_the_assessed_text_and_an_empty_selection_is_an_error() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write(
        "notes.txt",
        "alpha first line\nbravo second line\ncharlie third line\ndelta fourth line\necho fifth line\n",
    );
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let out = sections(
        &runtime,
        json!({
            "middle": {"tool":"localFetch","query":{"path":"notes.txt","ranges":[{"startLine":2,"endLine":3},{"startLine":3,"endLine":4}]}},
            "past": {"tool":"localFetch","query":{"path":"notes.txt","ranges":[{"startLine":10,"endLine":12}]}},
            "gone": {"tool":"localFetch","query":{"path":"missing.txt"}},
        }),
    )
    .await;
    let middle = &out["resources"]["middle"];
    assert_eq!(middle["status"], "ready", "{out:#}");
    assert_eq!(
        middle["sections"][0]["target"]["lines"],
        json!({"startLine":2,"endLine":4}),
        "overlapping ranges are one selection"
    );
    let past = &out["resources"]["past"];
    assert_eq!(past["status"], "error", "{out:#}");
    assert_eq!(error_code(past), "classificationContextEmpty");
    let gone = &out["resources"]["gone"];
    assert_eq!(gone["status"], "error", "{out:#}");
    assert!(gone["sections"].is_null() || gone["sections"] == json!([]));
    let sent = judged(&provider).await;
    assert!(sent.contains("bravo second line") && sent.contains("delta fourth line"));
    assert!(
        !sent.contains("alpha first") && !sent.contains("echo fifth"),
        "{sent}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn partial_redacted_context_is_unsupported_without_blocking_healthy_sections() {
    let provider = provider().await;
    let workspace = Workspace::new();
    let credential = ["https://", "fixture-user:fixture-password@", "example.com"].concat();
    let first = format!("{} {credential}\n", "x".repeat(7_900));
    let second = format!("dependent-target {}\n", "y".repeat(600));
    workspace.write("context.txt", format!("{first}{second}"));
    workspace.write("healthy.txt", "Independent restart instructions.\n");
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let out = sections(
        &runtime,
        json!({
            "context": {"tool":"localFetch","query":{"path":"context.txt"}},
            "healthy": {"tool":"localFetch","query":{"path":"healthy.txt"}},
        }),
    )
    .await;
    let planned = out["resources"]["context"]["sections"]
        .as_array()
        .expect("both source sections are reported");
    assert_eq!(planned.len(), 2, "{out:#}");
    assert_eq!(planned[0]["answers"]["q"]["status"], "answered", "{out:#}");
    assert_eq!(
        planned[1]["answers"]["q"]["status"], "unsupported",
        "{out:#}"
    );
    assert_eq!(
        planned[1]["answers"]["q"]["error"]["errorCode"],
        "classificationContextUnavailable"
    );
    let hints = planned[1]["answers"]["q"]["error"]["hints"]
        .as_array()
        .expect("actionable redaction recovery");
    assert!(hints.iter().any(|hint| {
        hint.as_str().is_some_and(|text| {
            text.contains("sanitized source") && text.contains("complete lines")
        })
    }));
    let hint_text = hints.iter().filter_map(Value::as_str).collect::<String>();
    assert!(!hint_text.contains("persistent storage") && !hint_text.contains("expired"));
    assert!(
        !planned[1]["context"]
            .as_array()
            .expect("planned context")
            .is_empty()
    );
    assert_eq!(
        out["resources"]["healthy"]["sections"][0]["answers"]["q"]["status"],
        "answered"
    );
    let sent = judged(&provider).await;
    assert!(
        sent.contains("[REDACTED"),
        "the complete redacted first line remains assessable"
    );
    assert!(sent.contains("Independent restart instructions."));
    assert!(
        !sent.contains("dependent-target"),
        "incomplete context must not reach inference"
    );
    assert!(!sent.contains("fixture-password"));
    assert!(!out.to_string().contains("fixture-password"));
    runtime.close().await;
}
