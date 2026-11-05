//! `classification` judges each hit: a mock provider that answers per target
//! line keeps exactly the hits whose own line says KEEP, whatever their
//! context windows touch. No real provider is involved.

use crate::support::{self, Workspace};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// `TARGET-LINE` when `question` names a target, and the state's text of that
/// line; otherwise the whole state: what a judge may look at.
fn judged_text(state: &Value, question: &Value) -> String {
    let target = question["instructions"]["target"].as_str().unwrap_or("");
    let line = target.strip_prefix("line ").map(|rest| {
        rest.split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or("")
    });
    let Some(line) = line.and_then(|line| line.parse::<u64>().ok()) else {
        return state.to_string();
    };
    let lines = state["lines"].as_str().unwrap_or_default();
    lines
        .lines()
        .find(|row| row.split('\t').next().and_then(|n| n.parse::<u64>().ok()) == Some(line))
        .unwrap_or_default()
        .to_owned()
}

fn verdict(state: &Value, question: &Value) -> Value {
    let levels = question["criteria"].as_array().expect("levels");
    let top = usize::from(judged_text(state, question).contains("KEEP")) * (levels.len() - 1);
    let legend = levels
        .iter()
        .enumerate()
        .map(|(index, level)| (index.to_string(), level.clone()))
        .collect::<serde_json::Map<_, _>>();
    let probabilities = (0..levels.len())
        .map(|index| {
            (
                index.to_string(),
                json!(if index == top { 1.0 } else { 0.0 }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({"type":"score","score":top as f64,"confidence":1.0,"legend":legend,"probabilities":probabilities})
}

fn answer(request: &Request) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).expect("provider body");
    let answers = body["questions"]
        .as_object()
        .expect("questions")
        .iter()
        .map(|(id, question)| (id.clone(), verdict(&body["state"], question)))
        .collect::<serde_json::Map<_, _>>();
    ResponseTemplate::new(200).set_body_json(
        json!({"model":"m","answers":answers,"usage":{"input_tokens":10,"output_tokens":1}}),
    )
}

async fn provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

/// Six hits, one line apart, so any context window touches its neighbour;
/// lines 1, 5 and 9 say KEEP.
const DENSE: &str = "needle KEEP one\n\nneedle drop two\n\nneedle KEEP three\n\nneedle drop four\n\nneedle KEEP five\n\nneedle drop six\n";

fn kept_hit_lines(data: &Value) -> Vec<u64> {
    let mut lines = data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["matches"].as_array().into_iter().flatten())
        .flat_map(|row| match row["matchedLines"].as_array() {
            Some(lines) => lines.iter().filter_map(Value::as_u64).collect(),
            None => vec![row["line"].as_u64().unwrap_or(0)],
        })
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

const PREDICATE: &str = "Does this line say KEEP?";

/// The hit is the unit whatever the context: a kept line satisfies the
/// predicate, and the receipt counts hits.
#[tokio::test]
async fn local_search_keeps_only_the_hits_that_pass_at_any_context() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    for context in [0, 1, 2, 3] {
        let row = support::run(
            &runtime,
            "localSearch",
            json!({"path":file,"matchString":["needle"],
                "contextLines":{"before":context,"after":context},
                "classification":{"predicates":[PREDICATE]}}),
        )
        .await;
        let data = row.data();
        assert_eq!(kept_hit_lines(data), [1, 5, 9], "context {context}: {data}");
        let receipt = &data["selection"];
        assert_eq!(receipt["unit"], "hits", "{data}");
        assert_eq!(receipt["totalItems"], 6, "{data}");
        assert_eq!(receipt["selectedItems"], 3, "{data}");
        assert_eq!(receipt["excludedItems"], 3, "{data}");
        assert_eq!(receipt["status"], "complete", "{data}");
    }
    runtime.close().await;
}

/// Hits of one file share one provider request: one call, one question per
/// hit, the state sent once.
#[tokio::test]
async fn hits_of_one_file_are_one_provider_call_with_one_question_each() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let row = support::run(
        &runtime,
        "localSearch",
        json!({"path":file,"matchString":["needle"],"contextLines":{"before":3,"after":3},
            "classification":{"predicates":[PREDICATE],"context":"RESEARCH_TASK"}}),
    )
    .await;
    assert_eq!(
        row.data()["selection"]["usage"]["calls"],
        1,
        "{}",
        row.data()
    );
    let sent = crate::runtime_clasify::sent(&server).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    let questions = sent[0]["questions"].as_object().expect("questions");
    assert_eq!(questions.len(), 6, "{sent:?}");
    // Task context goes once with the state, never into every question.
    assert_eq!(
        sent[0].to_string().matches("RESEARCH_TASK").count(),
        1,
        "{sent:?}"
    );
    runtime.close().await;
}

/// A matchString read judges its hits; a kept hit brings only its own window.
#[tokio::test]
async fn local_fetch_match_read_judges_each_hit() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let row = support::run(
        &runtime,
        "localFetch",
        json!({"path":file,"matchString":["needle"],"contextLines":{"before":0,"after":0},
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = row.data();
    let content = data["content"].as_str().unwrap_or_default();
    assert!(
        content.contains("KEEP one") && content.contains("KEEP five"),
        "{data}"
    );
    assert!(!content.contains("drop"), "{data}");
    let receipt = &data["selection"];
    assert_eq!(
        (
            &receipt["unit"],
            &receipt["totalItems"],
            &receipt["selectedItems"]
        ),
        (&json!("hits"), &json!(6), &json!(3)),
        "{data}"
    );
    let touching = support::run(
        &runtime,
        "localFetch",
        json!({"path":file,"matchString":["needle"],"contextLines":{"before":2,"after":2},
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = touching.data();
    assert_eq!(data["selection"]["selectedItems"], 3, "{data}");
    assert_eq!(data["selection"]["excludedItems"], 3, "{data}");
    runtime.close().await;
}

/// The whole read has no hits: its chunks stay the unit, and it says so.
#[tokio::test]
async fn a_read_without_a_search_counts_chunks() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let row = support::run(
        &runtime,
        "localFetch",
        json!({"path":file,"classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    assert_eq!(row.data()["selection"]["unit"], "chunks", "{}", row.data());
    runtime.close().await;
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn github_file(github: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/octo/demo/contents/dense.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"type":"file","encoding":"base64","content":STANDARD.encode(DENSE)}),
        ))
        .mount(github)
        .await;
}

/// The same hit rule on GitHub: a matchString read and a repo-scoped search.
#[tokio::test]
async fn github_reads_and_searches_judge_each_hit() {
    let server = provider().await;
    let github = MockServer::builder().start().await;
    github_file(&github).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).insert_header("etag", "\"page\"").set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[{
                "name":"dense.txt","path":"dense.txt","sha":"6",
                "html_url":format!("https://github.com/octo/demo/blob/{SHA}/dense.txt"),
                "repository":{"full_name":"octo/demo","html_url":"https://x","url":"https://x"},
                "text_matches":[{"fragment":"needle","matches":[{"text":"needle","indices":[0,6]}]}]}]
        })))
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let runtime = support::clasify_runtime(&workspace, &server, &support::github_api(&github));
    let read = support::run(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"octo","repo":"demo","path":"dense.txt","ref":SHA,"matchString":["needle"],
            "contextLines":{"before":2,"after":2},"classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = read.data();
    assert_eq!(data["selection"]["unit"], "hits", "{data}");
    assert_eq!(data["selection"]["totalItems"], 6, "{data}");
    assert_eq!(data["selection"]["selectedItems"], 3, "{data}");
    let search = support::run(
        &runtime,
        "ghSearchCode",
        json!({"owner":"octo","repo":"demo","keywords":["needle"],"ref":SHA,
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = search.data();
    assert_eq!(data["selection"]["unit"], "hits", "{data}");
    assert_eq!(data["selection"]["totalItems"], 6, "{data}");
    assert_eq!(data["selection"]["selectedItems"], 3, "{data}");
    let lines = data["files"][0]["lines"].as_array().expect("kept lines");
    assert!(
        lines
            .iter()
            .all(|line| line.as_str().is_some_and(|line| line.contains("KEEP"))),
        "{data}"
    );
    runtime.close().await;
}

/// A large judged search walks to the end: every hit decided exactly once,
/// its receipts adding up, with the rest and its cost visible on each page.
#[tokio::test]
async fn a_large_search_walks_every_hit_once() {
    let server = provider().await;
    let workspace = Workspace::new();
    let mut body = String::new();
    for index in 0..900 {
        let word = if index % 3 == 0 { "KEEP" } else { "drop" };
        body.push_str(&format!("needle {word} {index}\n\n"));
    }
    let file = workspace.write("large.txt", &body);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let mut query = json!({"path":file,"matchString":["needle"],
        "contextLines":{"before":0,"after":0},"classification":{"predicates":[PREDICATE]}});
    let (mut kept, mut decided) = (Vec::new(), 0);
    let mut pages = 0;
    loop {
        pages += 1;
        assert!(pages < 500, "the walk does not end");
        let row = support::run(&runtime, "localSearch", query).await;
        let data = row.data().clone();
        kept.extend(kept_hit_lines(&data));
        // Finish the output pages of this judged page before the next one.
        if let Some(next) = data["next"].get("nextPage") {
            query = next["input"].clone();
            continue;
        }
        let receipt = &data["selection"];
        decided +=
            receipt["selectedItems"].as_u64().unwrap() + receipt["excludedItems"].as_u64().unwrap();
        let Some(next) = data["next"].get("nextCandidatePage") else {
            break;
        };
        let rest = receipt["unknownItems"].as_u64().unwrap();
        assert!(rest > 0, "{receipt}");
        assert!(
            receipt["usage"]
                .get("remainingInputTokensEstimate")
                .is_some()
                || receipt["usage"]["inputTokens"] == 0,
            "cost of the rest is visible: {receipt}"
        );
        query = next["input"].clone();
    }
    assert_eq!(decided, 900);
    let expected = (0..900u64)
        .step_by(3)
        .map(|index| index * 2 + 1)
        .collect::<Vec<_>>();
    kept.sort_unstable();
    assert_eq!(kept, expected, "every kept hit once");
    runtime.close().await;
}

mod provider_faults;
