//! What bounds a row's provider requests before any is sent: its
//! `maxRequests`, which admits a first round that fits it, or, without one,
//! the sources, resources and items it names, one request each. A plan whose
//! own expansion passes that bound sends nothing, states its price, and
//! offers `next.confirm`, which runs it.

use super::{clasify, noul, sent};
use crate::support::{Workspace, clasify_runtime};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A `maxRequests` past every plan the other clasify tests make: they check
/// what a row judges, not what bounds it.
pub(crate) const EVERY_PAGE: u64 = 1_000;

/// Answers every question no (0.03), reporting its tokens.
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
                .map(|id| (id.clone(), json!({"type":"noul","noul":0.03})))
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-test","answers":answers,
                "usage":{"input_tokens":40,"output_tokens":4},
            }))
        })
        .mount(&server)
        .await;
    server
}

/// Three `needle` lines.
fn hits() -> String {
    (1..=3)
        .map(|line| format!("needle entry {line}\n"))
        .collect()
}

fn row(source: Value) -> Value {
    json!({"operation":"evaluate","source":source,
        "questions":{"q":noul("The entry names a restart.")}})
}

fn search(path: &str) -> Value {
    json!({"tool":"localSearch","query":{"path":path,"matchString":["needle"]}})
}

/// Runs an emitted call unchanged.
async fn run(runtime: &octocode_native::runtime::ToolRuntime, call: &Value) -> Value {
    crate::support::Run::run(
        runtime,
        "next".into(),
        call["tool"].as_str().expect("tool").to_owned(),
        call["input"].clone(),
    )
    .await
    .expect("the call runs")
    .structured_content
    .into_value()
}

/// The refusal of a plan its bound does not hold: nothing sent, its price,
/// and a confirm call at its ceiling.
fn assert_refused(data: &Value) -> u64 {
    assert_eq!(data["errorCode"], "classificationOverBudget", "{data}");
    assert_eq!(data["usage"]["calls"], 0, "{data}");
    let ceiling = data["usage"]["requestsCeiling"]
        .as_u64()
        .expect("a ceiling");
    let planned = data["usage"]["requestsPlanned"].as_u64().expect("a plan");
    assert!(planned <= ceiling, "{data}");
    assert_eq!(data["next"]["confirm"]["tool"], "clasify", "{data}");
    planned
}

/// C1: a search source over several files, a glob over several files and
/// `perHit` over several hits expand past the one source or key the row
/// names: without `maxRequests` they send nothing, and their confirm runs
/// them at their first round. What a row names runs as it did: one file's
/// hits (one request), listed resources and `perItem` items.
#[tokio::test]
async fn without_max_requests_a_row_sends_only_what_it_names() {
    let server = provider().await;
    let workspace = Workspace::new();
    for file in ["d/a.txt", "d/b.txt", "d/c.txt"] {
        workspace.write(file, hits());
    }
    let runtime = clasify_runtime(&workspace, &server, &[]);

    let out = clasify(&runtime, vec![row(search("d"))]).await;
    let data = &out["results"][0]["data"];
    let planned = assert_refused(data);
    assert_eq!(planned, 3, "one request per file: {out}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|error| error.contains("names")),
        "{out}"
    );
    assert!(sent(&server).await.is_empty(), "nothing was sent");
    let confirmed = run(&runtime, &data["next"]["confirm"]).await;
    let again = &confirmed["results"][0]["data"];
    assert!(again.get("errorCode").is_none(), "{confirmed}");
    assert_eq!(again["usage"]["calls"], planned, "{confirmed}");
    assert_eq!(sent(&server).await.len() as u64, planned);

    let before = sent(&server).await.len();
    let one = clasify(&runtime, vec![row(search("d/a.txt"))]).await;
    assert!(
        one["results"][0]["data"].get("errorCode").is_none(),
        "{one}"
    );
    assert_eq!(
        sent(&server).await.len() - before,
        1,
        "one file, one request"
    );

    let mut each = row(search("d/a.txt"));
    each["perHit"] = json!(true);
    let before = sent(&server).await.len();
    let out = clasify(&runtime, vec![each]).await;
    assert_eq!(assert_refused(&out["results"][0]["data"]), 3, "{out}");
    assert_eq!(sent(&server).await.len(), before, "nothing was sent");

    let resources = |resources: Value| {
        json!({"operation":"evaluate","resources":resources,
            "questions":{"q":noul("The entry names a restart.")}})
    };
    let glob = resources(json!({"d":{"tool":"localFetch","query":{"path":"d/*.txt"}}}));
    let out = clasify(&runtime, vec![glob]).await;
    assert_eq!(assert_refused(&out["results"][0]["data"]), 3, "{out}");
    assert_eq!(sent(&server).await.len(), before, "nothing was sent");
    let listed = resources(json!({
        "a":{"tool":"localFetch","query":{"path":"d/a.txt"}},
        "b":{"tool":"localFetch","query":{"path":"d/b.txt"}},
    }));
    let out = clasify(&runtime, vec![listed]).await;
    assert!(
        out["results"][0]["data"].get("errorCode").is_none(),
        "{out}"
    );
    assert_eq!(sent(&server).await.len() - before, 2, "one per named key");

    let before = sent(&server).await.len();
    let items = json!({"operation":"evaluate","perItem":true,
        "content":["restart it","leave it","stop it"],
        "questions":{"q":noul("The entry names a restart.")}});
    let out = clasify(&runtime, vec![items]).await;
    assert!(
        out["results"][0]["data"].get("errorCode").is_none(),
        "{out}"
    );
    assert!(
        sent(&server).await.len() - before <= 3,
        "at most one per item"
    );
    runtime.close().await;
}

/// C4: `maxRequests` admits a first round that fits it, however many
/// re-asks a missing answer could add: one file's hits share one request,
/// so `maxRequests: 1` runs it, held to the cap. A first round past the cap
/// sends nothing.
#[tokio::test]
async fn max_requests_admits_a_first_round_that_fits_and_holds_it() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("d/a.txt", hits());
    workspace.write("d/b.txt", hits());
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut capped = row(search("d/a.txt"));
    capped["maxRequests"] = json!(1);
    let out = clasify(&runtime, vec![capped]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");
    assert_eq!(data["usage"]["calls"], 1, "{out}");
    let ceiling = data["usage"]["requestsCeiling"].as_u64().unwrap_or(1);
    assert!(ceiling <= 1, "the ceiling never passes the cap: {out}");
    assert_eq!(sent(&server).await.len(), 1);

    let mut over = row(search("d"));
    over["maxRequests"] = json!(1);
    let out = clasify(&runtime, vec![over]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(assert_refused(data), 2, "{out}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|error| error.contains("maxRequests is 1")),
        "{out}"
    );
    assert_eq!(sent(&server).await.len(), 1, "nothing more was sent");
    runtime.close().await;
}

/// C6: `perHit` sends each hit of a search alone; a file or URL source has
/// no hits, so the row is refused before anything is read or sent.
#[tokio::test]
async fn per_hit_on_a_text_source_is_refused_before_anything_is_sent() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("a.txt", hits());
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut each = row(json!({"tool":"localFetch","query":{"path":"a.txt"}}));
    each["perHit"] = json!(true);
    each["maxRequests"] = json!(1);
    let out = clasify(&runtime, vec![each]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(data["errorCode"], "invalidClassificationContext", "{out}");
    assert!(data.get("usage").is_none(), "{out}");
    assert!(sent(&server).await.is_empty(), "nothing was sent");
    runtime.close().await;
}
