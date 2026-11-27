//! Provider failures: failed rows, recovery, and classification under a down provider or a refused credential.

use super::{Typed, clasify, content_row, noul};
use crate::support::{Run as _, Workspace, clasify_runtime};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[tokio::test]
async fn a_failed_github_source_keeps_its_transport_error_and_fresh_read() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_BETA", "true".into()),
        ("OCTOCODE_TYPESAFE_KEY", "fixture-only".into()),
        (
            "OCTOCODE_TYPESAFE_HOST",
            crate::support::UNMOCKED_GITHUB_API_URL
                .trim_end_matches("/api/v3")
                .into(),
        ),
    ]);
    // The fixture's GitHub endpoint is a closed loopback port.
    let source = json!({"operation":"search","keywords":["fixture"]});
    let out = clasify(
        &runtime,
        vec![json!({"operation":"evaluate","maxRequests":1,
            "source":{"tool":"ghSearchRepo","query":source},
            "questions":{"q":noul("Does this repository mention the fixture?")}
        })],
    )
    .await;
    let row = &out["results"][0];
    let data = &row["data"];
    assert_eq!(row["status"], "error", "{out}");
    assert_eq!(data["errorCode"], "transport", "{out}");
    assert_eq!(data["errorClass"], "transient", "{out}");
    assert!(data["next"].get("retry").is_none(), "{out}");
    assert!(data.get("usage").is_none(), "no request was judged: {out}");
    let read = &data["related"][0];
    assert_eq!(read["tool"], "ghSearchRepo", "{out}");
    assert_eq!(read["input"], json!({"queries":[source]}), "{out}");
    let retried = runtime
        .run("read".into(), "ghSearchRepo".into(), read["input"].clone())
        .await
        .expect("the exported read runs as a public request")
        .structured_content
        .into_value();
    assert_eq!(retried["results"][0]["data"]["errorCode"], "transport");
    runtime.close().await;
}

#[tokio::test]
async fn a_provider_failure_fails_the_row_and_the_runtime_recovers() {
    let server = MockServer::builder().start().await;
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = attempts.clone();
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &Request| {
            if counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                // 403, not 401: a rejected credential is remembered for a
                // minute (`gate::AUTH_MEMO`), so a rerun would be refused.
                ResponseTemplate::new(403)
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

/// No models list exists locally to refuse an unknown model before
/// the call, so the provider's refusal (HTTP 400) names the model the row
/// asked for. An evaluate row is not a selection: its error row offers no
/// `next.unfiltered` read.
#[tokio::test]
async fn a_provider_refusal_names_the_requested_model_and_offers_no_unfiltered_lead() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut row = content_row();
    row["model"] = json!("jev-nonexistent-9");
    let out = clasify(&runtime, vec![row]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "classificationProviderError", "{out}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|message| message.contains("jev-nonexistent-9")),
        "{out}"
    );
    assert!(data["next"].get("unfiltered").is_none(), "{out}");
    runtime.close().await;
}
