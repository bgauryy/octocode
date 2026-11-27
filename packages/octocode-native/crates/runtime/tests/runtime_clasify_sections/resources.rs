//! How each resource kind is acquired: GitHub files at a pinned commit, URLs to internal addresses, and local ranges.

use super::{error_code, judged, provider, provider_calls, sections};
use crate::support::{Run as _, Workspace, clasify_runtime, github_api};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod reads;

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
        octocode_test_support::mock_http::list_in_trees(&github, "a", "b", &format!("docs/{file}"))
            .await;
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
    assert!(
        guide.get("coordinateSpace").is_none(),
        "original source: {guide}"
    );
    assert_eq!(
        guide["sourceDigest"], SHA,
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
        json_fixture(r#"{"startLine": 1, "endLine": 3}"#)
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
        json_fixture(
            r#"{
                "page": {"url": "https://127.0.0.1:9/guide"},
                "notes": {"tool": "localFetch", "query": {"path": "notes.txt"}}
            }"#,
        ),
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
        json_fixture(
            r#"{
                "middle": {
                    "tool": "localFetch",
                    "query": {
                        "path": "notes.txt",
                        "ranges": [
                            {"startLine": 2, "endLine": 3},
                            {"startLine": 3, "endLine": 4}
                        ]
                    }
                },
                "past": {
                    "tool": "localFetch",
                    "query": {
                        "path": "notes.txt",
                        "ranges": [{"startLine": 10, "endLine": 12}]
                    }
                },
                "gone": {"tool": "localFetch", "query": {"path": "missing.txt"}}
            }"#,
        ),
    )
    .await;
    let middle = &out["resources"]["middle"];
    assert_eq!(middle["status"], "ready", "{out:#}");
    assert_eq!(
        middle["sections"][0]["target"]["lines"],
        json_fixture(r#"{"startLine": 2, "endLine": 4}"#),
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
        json_fixture(
            r#"{
                "context": {"tool": "localFetch", "query": {"path": "context.txt"}},
                "healthy": {"tool": "localFetch", "query": {"path": "healthy.txt"}}
            }"#,
        ),
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
    assert!(!hint_text.contains("storage.mode") && !hint_text.contains("expired"));
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

/// RV-CLS F2: `next.quiet` serves the held-back sections of a GitHub plan
/// only to the GitHub identity that planned it. With the same credential it
/// replays them unbilled; after the credential changes the issued cursor is
/// refused before clasify runs, and no saved section is served. (The plan's
/// own identity check, for a token that reaches `sections::run`, is pinned by
/// the lib test `quiet_identity`.)
#[tokio::test]
async fn next_quiet_of_a_github_plan_checks_the_github_identity_first() {
    use octocode_runtime_base::providers::github::{CredentialStore, StoredCredentials};
    let provider = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json_fixture(r#"{"type": "noul", "noul": 0.1}"#)))
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-1.13.0","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}
            }))
        })
        .mount(&provider)
        .await;
    let github = github_with(&[("guide.md", b"# Guide\n\nRead the logs quietly.\n")]).await;
    let workspace = Workspace::new();
    let store = CredentialStore::new(&workspace.home);
    let save = |token: &str| {
        let credentials: StoredCredentials = serde_json::from_value(json!({
            "hostname":"127.0.0.1","username":"fixture","token":{"token":token,"tokenType":"oauth"},
            "gitProtocol":"https","createdAt":"","updatedAt":""
        }))
        .expect("stored credential fixture");
        store.save(&credentials).expect("save home credential");
    };
    save("first-local-token");
    let mut env = github_api(&github).to_vec();
    env.push(("GITHUB_TOKEN", String::new()));
    let runtime = clasify_runtime(&workspace, &provider, &env);
    let out = sections(&runtime, json!({"guide": github_source("guide.md")})).await;
    let guide = &out["resources"]["guide"];
    assert_eq!(guide["status"], "ready", "{out:#}");
    assert_eq!(guide["coverage"]["quiet"], json!(1), "{out:#}");
    let quiet = out["next"]["quiet"]["input"].clone();
    assert!(quiet.is_object(), "next.quiet: {out:#}");
    let same = runtime
        .run("quiet-same".into(), "clasify".into(), quiet.clone())
        .await
        .expect("the same identity replays")
        .structured_content
        .into_value();
    assert_eq!(
        same["results"][0]["data"]["resources"]["guide"]["sections"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the same identity reads the held section: {same:#}"
    );
    let calls = provider_calls(&provider).await;
    save("switched-local-token");
    let changed = runtime
        .run("quiet-switched".into(), "clasify".into(), quiet)
        .await
        .expect_err("a changed identity is refused");
    assert_eq!(
        changed.code,
        octocode_contract_types::ErrorCode::ResponseSnapshotExpired,
        "{changed:?}"
    );
    assert!(
        changed.payload.is_none(),
        "no saved section is served: {changed:?}"
    );
    assert_eq!(provider_calls(&provider).await, calls, "nothing is billed");
    runtime.close().await;
}

/// A local resource is named by the text a read shows of it: its row states
/// no digest, length or byte of the file's own text, so nothing shown tests a
/// guess of the value a read hides. Two files that differ only in that value
/// show the same digest and bytes; the plan's own pin still sees the change,
/// so a saved cursor over it is stale, and an edit a read shows changes the
/// digest.
#[tokio::test]
async fn a_local_resource_states_no_digest_or_byte_of_a_hidden_value() {
    // Fabricated tokens of two lengths: secret-shaped, never real.
    let short = "ghp_Fake0123456789abcdefFAKE0123456789ab";
    let long = "ghp_Fake0123456789abcdefFAKE0123456789abFAKE0123456789xyz";
    let file = |token: &str, mode: &str| {
        format!("# settings\nGITHUB_TOKEN={token}\nmode = {mode}\nrestart = never\n")
    };
    let raw = |text: &str| octocode_engine::digest::sha256(text.as_bytes());
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write("notes.txt", file(short, "fast"));
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let row = |view: &str| {
        json!({"operation":"sections","view":view,
            "resources":{"file":{"tool":"localFetch","query":{"path":"notes.txt"}}},
            "questions":{"q":{"type":"noul","instructions":"Does this section say to restart?"}}})
    };
    let run = |label: &'static str, input: Value| {
        let runtime = &runtime;
        async move {
            runtime
                .run(label.into(), "clasify".into(), input)
                .await
                .expect("clasify")
                .structured_content
                .into_value()
        }
    };
    let shown = |out: &Value| {
        let resource = &out["results"][0]["data"]["resources"]["file"];
        assert_eq!(resource["status"], "ready", "{out:#}");
        let bytes = resource["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .map(|section| section["target"]["bytes"].clone())
            .collect::<Vec<_>>();
        assert!(bytes.iter().all(Value::is_array), "{out:#}");
        (resource["sourceDigest"].clone(), bytes)
    };
    let first = run("first", json!({"queries":[row("details")]})).await;
    let text = first.to_string();
    assert!(!text.contains(short), "{text}");
    assert!(
        !text.contains(&raw(&file(short, "fast"))),
        "no digest of the file's bytes: {text}"
    );
    let (digest, bytes) = shown(&first);
    assert_eq!(digest.as_str().map(str::len), Some(64), "{first:#}");
    // The summary view offers a saved replay pinned to the judged file.
    let summary = run("summary", json!({"queries":[row("summary")]})).await;
    let replay = summary["results"][0]["data"]["next"]["details"]["input"].clone();
    crate::support::assert_cursor_only(&replay);

    // Only the hidden value changes, to one of another length.
    workspace.write("notes.txt", file(long, "fast"));
    let other = run("other", json!({"queries":[row("details")]})).await;
    assert!(!other.to_string().contains(long), "{other:#}");
    assert!(
        !other.to_string().contains(&raw(&file(long, "fast"))),
        "{other:#}"
    );
    assert_eq!(
        shown(&other),
        (digest.clone(), bytes),
        "the shown text is the same, so is all it states"
    );
    let stale = run("stale", replay).await;
    assert_eq!(
        stale["results"][0]["data"]["errorCode"], "staleSnapshot",
        "the pin sees what the shown digest does not: {stale:#}"
    );

    // An edit a read shows changes the digest.
    workspace.write("notes.txt", file(long, "slow"));
    let edited = run("edited", json!({"queries":[row("details")]})).await;
    assert_ne!(shown(&edited).0, digest, "{edited:#}");
}

#[tokio::test]
async fn classification_disclosures_glob_scope_and_listing_survive_details_replay() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write("src/public.rs", "pub fn restart() {}\n");
    workspace.write("src/.hidden.rs", "pub fn hidden() {}\n");
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let first = runtime
        .run(
            "scope-first".into(),
            "clasify".into(),
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "sections",
                            "view": "summary",
                            "resources": {"r": {"tool": "localFetch", "query": {"path": "src/*.rs"}}},
                            "questions": {
                                "q": {
                                    "type": "noul",
                                    "instructions": "Does this section say to restart?"
                                }
                            }
                        }
                    ]
                }"#,
            ),
        )
        .await
        .expect("summary with a saved details continuation")
        .structured_content
        .into_value();
    let first = &first["results"][0]["data"];
    assert!(
        first["warnings"]
            .as_array()
            .expect("source scope warning")
            .iter()
            .any(|warning| {
                warning
                    .as_str()
                    .is_some_and(|text| text.contains(".hidden.rs"))
            }),
        "{first}"
    );
    let calls = provider_calls(&provider).await;
    assert_eq!(first["next"]["details"]["tool"], "clasify", "{first}");
    let replay = runtime
        .run(
            "scope-replay".into(),
            "clasify".into(),
            first["next"]["details"]["input"].clone(),
        )
        .await
        .expect("saved details")
        .structured_content
        .into_value();
    let replay = &replay["results"][0]["data"];
    assert_eq!(replay["warnings"], first["warnings"], "{replay}");
    assert_eq!(replay["related"], first["related"], "{replay}");
    assert!(
        replay["related"]
            .as_array()
            .is_some_and(|calls| calls.iter().any(|call| call["tool"] == "structureSearch")),
        "{replay}"
    );
    assert_eq!(replay["usage"]["replayed"], true, "{replay}");
    assert_eq!(
        replay["usage"]["calls"].as_u64(),
        Some(calls as u64),
        "a replay reports the spend of the page it replays: {replay}"
    );
    assert_eq!(replay["replayed"], true, "{replay}");
    assert_eq!(
        provider_calls(&provider).await,
        calls,
        "replay submits nothing"
    );
    runtime.close().await;
}
