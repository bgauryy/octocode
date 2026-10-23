//! How each resource kind is acquired: GitHub files at a pinned commit, URLs to internal addresses, and local ranges.

use super::{error_code, judged, provider, provider_calls, sections};
use crate::support::{Workspace, clasify_runtime, github_api};
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
        "owner":"a","repo":"b","path":format!("docs/{file}"),"ref":"v1","fullContent":true}})
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
    assert_eq!(
        guide["read"]["query"]["queries"][0]["ref"], SHA,
        "the read lead returns exactly what was assessed"
    );
    assert_eq!(guide["sections"][0]["target"]["lines"], json!([1, 3]));
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
            "notes": {"tool":"localFetch","query":{"path":"notes.txt","fullContent":true}},
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
            "middle": {"tool":"localFetch","query":{"path":"notes.txt","ranges":["2-3","3-4"]}},
            "past": {"tool":"localFetch","query":{"path":"notes.txt","ranges":["10-12"]}},
            "gone": {"tool":"localFetch","query":{"path":"missing.txt","fullContent":true}},
        }),
    )
    .await;
    let middle = &out["resources"]["middle"];
    assert_eq!(middle["status"], "ready", "{out:#}");
    assert_eq!(
        middle["sections"][0]["target"]["lines"],
        json!([2, 4]),
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
