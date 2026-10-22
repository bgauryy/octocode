//! clasify `operation: sections`: how each kind of named resource is
//! acquired. Every failure is that resource's own row, found before any
//! provider call; a sibling keeps its sections.

use crate::runtime_clasify::{clasify, sent};
use crate::support::{PROVIDER_LIMITS, Run as _, Workspace, clasify_runtime, github_api};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const SHA: &str = "1111111111111111111111111111111111111111";

/// Answers every question Noul 0.9.
async fn provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json!({"type":"noul","noul":0.9})))
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-1.13.0","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}
            }))
        })
        .mount(&server)
        .await;
    server
}

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

async fn sections(runtime: &ToolRuntime, resources: Value) -> Value {
    let out = clasify(
        runtime,
        vec![json!({
            "id":"s","operation":"sections","resources":resources,
            "questions":{"q":{"type":"noul","instructions":"Does this section say to restart?"}}
        })],
    )
    .await;
    out["queries"][0].clone()
}

async fn provider_calls(server: &MockServer) -> usize {
    sent(server).await.len()
}

/// What the provider was asked to judge, one request body per call.
async fn judged(server: &MockServer) -> String {
    sent(server)
        .await
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn error_code(resource: &Value) -> &Value {
    &resource["error"]["errorCode"]
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

#[tokio::test]
async fn section_resume_and_replay_use_each_current_query_id() {
    let provider = provider().await;
    let workspace = Workspace::new();
    let source = (0..70)
        .map(|index| format!("pub fn section_{index}() {{ println!(\"restart {index}\"); }}\n"))
        .collect::<String>();
    workspace.write("sections.rs", &source);
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let first = sections(
        &runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"sections.rs","fullContent":true}}}),
    )
    .await;
    assert_eq!(first["id"], "s");
    let mut resume = first["next"]["nextSectionPage"]["query"]["queries"][0].clone();
    assert!(resume["assessment"].is_string(), "{first:#}");
    resume["id"] = json!("resume");
    let resumed = runtime
        .run(
            "resume-current-id".into(),
            "clasify".into(),
            json!({"queries":[resume.clone()]}),
        )
        .await
        .expect("resumed result")
        .structured_content;
    assert_eq!(resumed["queries"][0]["id"], "resume", "{resumed:#}");
    let calls = provider_calls(&provider).await;
    let replays = ["first-replay", "second-replay"].map(|id| {
        let mut query = resume.clone();
        query["id"] = json!(id);
        query
    });
    let replayed = runtime
        .run(
            "replay-current-ids".into(),
            "clasify".into(),
            json!({"queries":replays}),
        )
        .await
        .expect("replayed result")
        .structured_content;
    for (index, id) in ["first-replay", "second-replay"].iter().enumerate() {
        let row = &replayed["queries"][index];
        assert_eq!(row["id"], *id, "{replayed:#}");
        assert_eq!(row["replayed"], true, "{replayed:#}");
        assert_eq!(row["resources"], resumed["queries"][0]["resources"]);
    }
    assert_eq!(provider_calls(&provider).await, calls);
    runtime.close().await;
}

#[tokio::test]
async fn sibling_section_rows_keep_continuations_until_every_section_is_answered() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write(
        "many.rs",
        (0..70)
            .map(|i| format!("pub fn section_{i}() {{ println!(\"restart {i}\"); }}\n"))
            .collect::<String>(),
    );
    let runtime = clasify_runtime(
        &workspace,
        &provider,
        &[("OCTOCODE_STORAGE_MODE", "persistent".into())],
    );
    let rows = ["a", "b"].map(|id| json!({"id":id,"operation":"sections","resources":{"file":{"tool":"localFetch","query":{"path":"many.rs","fullContent":true}}},"questions":{"q":{"type":"noul","instructions":"Does this section say to restart?"}}}));
    let first = runtime
        .run(
            "shared-sections".into(),
            "clasify".into(),
            json!({"queries":rows}),
        )
        .await
        .expect("first batch")
        .structured_content;
    for initial in first["queries"].as_array().expect("rows") {
        let mut row = initial.clone();
        let mut seen = std::collections::HashSet::new();
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(pages <= 4, "continuation made no bounded progress: {row:#}");
            for section in row["resources"]["file"]["sections"]
                .as_array()
                .expect("sections")
            {
                assert_eq!(section["answers"]["q"]["status"], "answered", "{row:#}");
                assert!(
                    seen.insert(section["id"].as_str().expect("section id").to_owned()),
                    "section emitted twice"
                );
            }
            let next = &row["next"]["nextSectionPage"];
            if next.is_null() {
                break;
            }
            row = runtime
                .run(
                    format!("resume-{}-{pages}", initial["id"]),
                    "clasify".into(),
                    next["query"].clone(),
                )
                .await
                .expect("resume")
                .structured_content["queries"][0]
                .clone();
        }
        assert_eq!(seen.len(), 70, "{row:#}");
        assert_eq!(row["resources"]["file"]["coverage"]["unassessed"], 0);
    }
    runtime.close().await;
}

#[tokio::test]
async fn simultaneous_section_resume_in_independent_runtimes_replays_one_transaction() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write(
        "many.rs",
        (0..70)
            .map(|i| format!("pub fn section_{i}() {{ println!(\"restart {i}\"); }}\n"))
            .collect::<String>(),
    );
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let first_runtime = clasify_runtime(&workspace, &provider, &env);
    let second_runtime = clasify_runtime(&workspace, &provider, &env);
    let first = sections(
        &first_runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"many.rs","fullContent":true}}}),
    )
    .await;
    let next = first["next"]["nextSectionPage"]["query"].clone();
    assert!(next["queries"][0]["assessment"].is_string(), "{first:#}");
    let before = provider_calls(&provider).await;
    let (a, b) = tokio::join!(
        first_runtime.run("resume-a".into(), "clasify".into(), next.clone()),
        second_runtime.run("resume-b".into(), "clasify".into(), next)
    );
    let a = a.expect("first resume").structured_content;
    let b = b.expect("second resume").structured_content;
    assert_eq!(a["queries"][0]["resources"], b["queries"][0]["resources"]);
    assert_ne!(
        a["queries"][0]["replayed"] == true,
        b["queries"][0]["replayed"] == true,
        "{a:#} {b:#}"
    );
    assert_eq!(provider_calls(&provider).await - before, 6);
    first_runtime.close().await;
    second_runtime.close().await;
}

/// A provider that holds every request until `quorum` requests are on the
/// wire at once (or `patience` passes), then answers each question Noul 0.9.
/// `peak` is the most requests it ever held together.
struct Rendezvous {
    uri: String,
    state: Arc<(Mutex<RendezvousState>, Condvar)>,
}

#[derive(Default)]
struct RendezvousState {
    in_flight: usize,
    peak: usize,
}

impl Rendezvous {
    fn start(quorum: usize, patience: Duration) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind provider");
        let uri = format!(
            "http://{}",
            listener.local_addr().expect("provider address")
        );
        let state = Arc::new((Mutex::new(RendezvousState::default()), Condvar::new()));
        let shared = Arc::clone(&state);
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let state = Arc::clone(&shared);
                std::thread::spawn(move || Self::answer(stream, &state, quorum, patience));
            }
        });
        Self { uri, state }
    }

    fn answer(
        stream: std::net::TcpStream,
        state: &(Mutex<RendezvousState>, Condvar),
        quorum: usize,
        patience: Duration,
    ) {
        let mut reader = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let (lock, arrived) = state;
        let mut held = lock.lock().expect("provider state");
        held.in_flight += 1;
        held.peak = held.peak.max(held.in_flight);
        arrived.notify_all();
        let (mut held, _) = arrived
            .wait_timeout_while(held, patience, |held| held.peak < quorum)
            .expect("provider state");
        held.in_flight -= 1;
        drop(held);
        let request: Value = serde_json::from_slice(&body).expect("provider body");
        let answers = request["questions"]
            .as_object()
            .expect("questions")
            .keys()
            .map(|id| (id.clone(), json!({"type":"noul","noul":0.9})))
            .collect::<serde_json::Map<_, _>>();
        let response = json!({"model":"jev-1.13.0","answers":answers,
            "usage":{"input_tokens":10,"output_tokens":2}})
        .to_string();
        let mut stream = reader.into_inner();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response}",
            response.len()
        );
    }

    fn peak(&self) -> usize {
        self.state.0.lock().expect("provider state").peak
    }
}

#[tokio::test]
async fn admitted_sections_are_judged_concurrently_and_all_answer() {
    let sections_count = 4;
    let provider = Rendezvous::start(sections_count, Duration::from_secs(3));
    let workspace = Workspace::new();
    workspace.write(
        "four.rs",
        (0..sections_count)
            .map(|i| format!("pub fn section_{i}() {{ println!(\"restart {i}\"); }}\n"))
            .collect::<String>(),
    );
    let runtime = workspace.runtime_with(
        &[
            ("OCTOCODE_BETA", "true".to_owned()),
            ("OCTOCODE_CLASSIFICATION_API", "secret".to_owned()),
            ("OCTOCODE_CLASSIFICATION_API_HOST", provider.uri.clone()),
        ],
        PROVIDER_LIMITS,
    );
    let out = sections(
        &runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"four.rs","fullContent":true}}}),
    )
    .await;
    let file = &out["resources"]["file"];
    assert_eq!(file["coverage"]["totalSections"], sections_count, "{out:#}");
    assert_eq!(file["coverage"]["answered"], sections_count, "{out:#}");
    assert_eq!(
        provider.peak(),
        sections_count,
        "every admitted section is on the wire at once"
    );
    assert!(out.get("next").is_none(), "{out:#}");
    runtime.close().await;
}
