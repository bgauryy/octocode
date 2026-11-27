//! What bounds a row's provider requests before any is sent: its
//! `maxRequests`, which admits a first round that fits it, or, without one,
//! what the sources, resources and items it names could spend, one request
//! window each. A plan whose
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

/// C1: without `maxRequests` a row spends what the sources, resources and
/// items it names could, one provider window each. Before this process took
/// a request of the model, nothing bounds the provider's own prompt, so a
/// row needing more requests than it names first spends one on a request
/// with no evidence that measures it; then `perHit` over several hits and a
/// glob over several files cost less than their one named key could, and run. A search's hits share requests across files,
/// so a search over several files is one request; listed resources and
/// `perItem` items run as named.
#[tokio::test]
async fn without_max_requests_a_row_spends_only_what_it_names() {
    let server = provider().await;
    let workspace = Workspace::new();
    for file in ["d/a.txt", "d/b.txt", "d/c.txt"] {
        workspace.write(file, hits());
    }
    let runtime = clasify_runtime(&workspace, &server, &[]);

    // Nothing learned yet: `perHit` over three hits spends one of its named
    // requests on a request with no evidence, is priced again within what
    // that one key could cost, and runs.
    let mut each = row(search("d/a.txt"));
    each["perHit"] = json!(true);
    let out = clasify(&runtime, vec![each]).await;
    let data = &out["results"][0]["data"];
    assert!(data.get("errorCode").is_none(), "{out}");
    assert_eq!(
        data["usage"]["calls"], 4,
        "three hits and the learning request: {out}"
    );
    let warnings = data["warnings"].to_string();
    assert!(warnings.contains("no evidence"), "{out}");
    assert!(!warnings.contains("past requestsCeiling"), "{out}");
    let bodies = sent(&server).await;
    assert_eq!(bodies.len(), 4, "{out}");
    assert!(
        !bodies[0].to_string().contains("needle entry"),
        "the learning request holds no evidence"
    );

    let resources = |resources: Value| {
        json!({"operation":"evaluate","resources":resources,
            "questions":{"q":noul("The entry names a restart.")}})
    };
    let before = sent(&server).await.len();
    let glob = resources(json!({"d":{"tool":"localFetch","query":{"path":"d/*.txt"}}}));
    let out = clasify(&runtime, vec![glob]).await;
    assert!(
        out["results"][0]["data"].get("errorCode").is_none(),
        "within what its one key could spend: {out}"
    );
    assert_eq!(sent(&server).await.len() - before, 3, "one per file");

    let before = sent(&server).await.len();
    let out = clasify(&runtime, vec![row(search("d"))]).await;
    assert!(
        out["results"][0]["data"].get("errorCode").is_none(),
        "{out}"
    );
    assert_eq!(
        sent(&server).await.len() - before,
        1,
        "every file's hits in one request"
    );

    let before = sent(&server).await.len();
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
/// re-asks a missing answer could add: a search's hits share one request,
/// so `maxRequests: 1` runs it, held to the cap. A first round past the cap
/// (each hit alone) sends nothing, whatever it costs.
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

    let mut over = row(search("d/b.txt"));
    over["perHit"] = json!(true);
    over["maxRequests"] = json!(1);
    let out = clasify(&runtime, vec![over]).await;
    let data = &out["results"][0]["data"];
    assert_eq!(assert_refused(data), 3, "{out}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|error| error.contains("maxRequests is 1")),
        "{out}"
    );
    assert_eq!(sent(&server).await.len(), 1, "nothing more was sent");
    // Its confirm runs it at its ceiling.
    let confirmed = run(&runtime, &data["next"]["confirm"]).await;
    let again = &confirmed["results"][0]["data"];
    assert!(again.get("errorCode").is_none(), "{confirmed}");
    assert_eq!(again["usage"]["calls"], 3, "{confirmed}");
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
