//! Provider failures: failed rows, recovery, and classification under a down provider or a refused credential.

use super::{Typed, clasify, content_row, noul};
use crate::support::{Run as _, Workspace, clasify_runtime};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[tokio::test]
async fn a_provider_failure_fails_the_row_and_the_runtime_recovers() {
    let server = MockServer::builder().start().await;
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = attempts.clone();
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &Request| {
            if counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(401)
            } else {
                Typed.respond(request)
            }
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let failed = clasify(&runtime, vec![content_row()]).await;
    assert_eq!(
        failed["results"][0]["data"]["errorCode"], "classificationProviderError",
        "{failed}"
    );
    assert!(failed["results"][0]["data"].get("answers").is_none());
    let recovered = clasify(&runtime, vec![content_row()]).await;
    assert_eq!(
        recovered["results"][0]["data"]["answers"]["restart"]["noul"],
        0.9
    );
    assert!(
        recovered["results"][0]["data"].get("replayed").is_none(),
        "a failure is never cached"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_provider_failure_keeps_the_pinned_source_inspection_call() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "src/evidence.txt",
        "RECOVERY_SOURCE_SENTINEL applies after restart.\n",
    );
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(
        &runtime,
        vec![json!({"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"src/evidence.txt","ranges":[{"startLine":1,"endLine":1}]}},
            "questions":{"restart":noul("Does this state a restart requirement?")}
        })],
    )
    .await;
    let row = &out["results"][0]["data"];
    assert_eq!(row["errorCode"], "classificationProviderError");
    assert!(row.get("answers").is_none());
    assert!(!out.to_string().contains("RECOVERY_SOURCE_SENTINEL"));
    let lead = &row["next"]["read"];
    assert_eq!(lead["tool"], "localFetch", "{out}");
    crate::support::assert_cursor_only(&lead["input"]);
    assert!(
        row["hints"]
            .as_array()
            .is_some_and(|hints| !hints.is_empty())
    );
    let read = runtime
        .run("inspect".into(), "localFetch".into(), lead["input"].clone())
        .await
        .expect("inspection call");
    assert!(
        read.structured_content
            .to_string()
            .contains("RECOVERY_SOURCE_SENTINEL")
    );
    runtime.close().await;
}

/// A provider that is down (503 without a retry delay) cannot judge the
/// search, but the search succeeded: its hits come back unfiltered under an
/// `unavailable` receipt and a warning naming the failure, unlike a refused
/// credential (below), which the caller must fix and so stays the row's
/// error.
#[tokio::test]
async fn a_provider_that_is_down_under_clasify_context_returns_the_unfiltered_hits() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("src/a.rs", "fn recovery() {}\nfn other() { recovery(); }\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = crate::support::run(
        &runtime,
        "localSearch",
        json!({"path": file.parent().unwrap(), "matchString":["recovery"],
            "classification": {"predicates": ["Does it add hints?"]}}),
    )
    .await;
    let data = row.data();
    assert_ne!(row.status(), "error", "{data}");
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(
        data["selection"]["reason"], "classificationProviderUnavailable",
        "{data}"
    );
    let text = data.to_string();
    assert!(
        text.contains("fn recovery() {}"),
        "every hit is shown: {data}"
    );
    assert!(text.contains("other() { recovery(); }"), "{data}");
    assert!(
        text.contains("HTTP 503"),
        "the warning names the failure: {data}"
    );
    runtime.close().await;
}

/// The same for a read and a provider page: localFetch, ghGetFileContent
/// and ghSearchCode show their evidence unfiltered under an `unavailable`
/// receipt and a warning naming the failure, with no unfiltered lead that
/// repeats it.
#[tokio::test]
async fn a_provider_that_is_down_under_clasify_context_shows_reads_and_code_pages_unfiltered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let github = MockServer::builder().start().await;
    let sha = "1111111111111111111111111111111111111111";
    let body = "fn recovery() {}\nfn other() { recovery(); }\n";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode(body),
            "size":body.len(),"sha":"2".repeat(40),"path":"src/a.rs"
        })))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"a.rs","path":"src/a.rs","sha":"2","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn recovery() {}","matches":[{"text":"recovery","indices":[3,11]}]}]}
            ]
        })))
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("src/a.rs", body);
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[("GITHUB_API_URL", format!("{}/api/v3", github.uri()))],
    );
    let context = json!({"predicates": ["Does it add hints?"]});
    for (tool, query, shown) in [
        (
            "localFetch",
            json!({"path": file, "classification": context}),
            "other() { recovery(); }",
        ),
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"src/a.rs","ref":"main",
                "classification": context}),
            "other() { recovery(); }",
        ),
        (
            "ghSearchCode",
            json!({"owner":"a","repo":"b","keywords":["recovery"], "classification": context}),
            "src/a.rs",
        ),
    ] {
        let row = crate::support::run(&runtime, tool, query).await;
        let data = row.data();
        assert_ne!(row.status(), "error", "{tool}: {data}");
        assert_eq!(data["selection"]["status"], "unavailable", "{tool}: {data}");
        assert_eq!(
            data["selection"]["reason"], "classificationProviderUnavailable",
            "{tool}: {data}"
        );
        assert!(
            data.to_string().contains(shown),
            "{tool}: the evidence is shown: {data}"
        );
        assert!(
            data["warnings"].to_string().contains("HTTP 503"),
            "{tool}: the warning names the failure: {data}"
        );
        assert!(
            data["next"].get("unfiltered").is_none(),
            "{tool}: no lead repeats the page: {data}"
        );
    }
    runtime.close().await;
}

/// A provider that refuses the credential is not an unsure model: under
/// classification the row fails with the provider's typed error, its
/// credential hint, an `unavailable` receipt, and the unfiltered lead, and
/// the call exits as a failure instead of success.
#[tokio::test]
async fn a_refused_credential_under_clasify_context_fails_the_row_with_its_error() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("src/a.rs", "fn recovery() {}\nfn other() { recovery(); }\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = crate::support::run(
        &runtime,
        "localSearch",
        json!({"path": file.parent().unwrap(), "matchString":["recovery"],
            "classification": {"predicates": ["Does it add hints?"]}}),
    )
    .await;
    assert_eq!(row.status(), "error", "{}", row.data());
    let data = row.data();
    assert_eq!(data["errorCode"], "classificationProviderError", "{data}");
    assert!(
        data["hints"]
            .to_string()
            .contains("OCTOCODE_CLASSIFICATION_API"),
        "{data}"
    );
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(
        data["selection"]["reason"], "classificationProviderError",
        "{data}"
    );
    assert!(data["next"]["unfiltered"].is_object(), "{data}");
    assert_ne!(
        row.outcome.exit_class(),
        octocode_native::response::exit::ExitClass::Success,
        "{data}"
    );
    runtime.close().await;
}
