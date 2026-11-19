//! `classification` judges each hit: a mock provider that answers per target
//! line keeps exactly the hits whose own line says KEEP, whatever their
//! context windows touch. No real provider is involved.

use crate::support::{self, Workspace};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// `TARGET-LINE` when `question` names a target, and the state's text of that
/// line, else the text its label quotes; otherwise the whole state: what a
/// judge may look at.
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
        .or_else(|| target.split_once(": ").map(|(_, quoted)| quoted))
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

pub(super) async fn provider() -> MockServer {
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
pub(super) const DENSE: &str = "needle KEEP one\n\nneedle drop two\n\nneedle KEEP three\n\nneedle drop four\n\nneedle KEEP five\n\nneedle drop six\n";

pub(super) fn kept_hit_lines(data: &Value) -> Vec<u64> {
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

pub(super) const PREDICATE: &str = "Does this line say KEEP?";

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
    // So does the question every target shares; each question carries only
    // its own target and the classifier.
    assert_eq!(
        sent[0].to_string().matches("target line itself").count(),
        1,
        "{sent:?}"
    );
    for question in questions.values() {
        assert!(
            question["instructions"].get("question").is_none(),
            "{question}"
        );
    }
    runtime.close().await;
}

/// Each source line reaches the shared state once: every hit's own line and
/// each context line, even one that informs two neighbours. Each question's
/// label names its hit by line number and quotes it (B58 measures dropping
/// the quote), so a hit's text appears once in the state and once in its
/// own label, never in another hit's.
#[tokio::test]
async fn each_hit_line_is_sent_once() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    for context in [0, 1] {
        let row = support::run(
            &runtime,
            "localSearch",
            json!({"path":file,"matchString":["needle"],
                "contextLines":{"before":context,"after":context},
                "classification":{"predicates":[PREDICATE]}}),
        )
        .await;
        assert_eq!(kept_hit_lines(row.data()), [1, 5, 9], "{}", row.data());
    }
    let sent = crate::runtime_clasify::sent(&server).await;
    assert_eq!(sent.len(), 2, "{sent:?}");
    for (request, context) in sent.iter().zip([0, 1]) {
        let state = request["state"]["lines"].as_str().unwrap_or_default();
        let labels = request["questions"]
            .as_object()
            .expect("questions")
            .values()
            .map(|question| {
                question["instructions"]["target"]
                    .as_str()
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        for (line, text) in [
            (1, "needle KEEP one"),
            (3, "needle drop two"),
            (9, "needle KEEP five"),
        ] {
            assert_eq!(
                state
                    .lines()
                    .filter(|record| *record == format!("{line}\t{text}"))
                    .count(),
                1,
                "context {context}: the state holds line {line} once: {request}"
            );
            let quoting = labels
                .iter()
                .filter(|label| label.contains(text))
                .collect::<Vec<_>>();
            assert_eq!(
                quoting,
                [&format!("line {line}: {text}")],
                "context {context}: only its own label quotes line {line}: {request}"
            );
            assert_eq!(
                request.to_string().matches(text).count(),
                2,
                "context {context}: state and label, nowhere else: {request}"
            );
        }
    }
    // With one line of context, line 2 (blank) informs both neighbours and
    // is still sent once.
    let state = sent[1]["state"]["lines"].as_str().unwrap_or_default();
    assert_eq!(
        state.lines().filter(|line| line.starts_with("2\t")).count(),
        1,
        "{:?}",
        sent[1]
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

/// A classified read says what it kept, and that keeping none is not absence.
#[tokio::test]
async fn a_classified_read_warns_what_it_kept_and_that_empty_is_not_absence() {
    let server = provider().await;
    let workspace = Workspace::new();
    let dense = workspace.write("dense.txt", DENSE);
    let none = workspace.write("none.txt", "needle drop one\n\nneedle drop two\n");
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let read = async |file: &std::path::Path| {
        support::run(
            &runtime,
            "localFetch",
            json!({"path":file,"matchString":["needle"],"contextLines":{"before":0,"after":0},
                "classification":{"predicates":[PREDICATE]}}),
        )
        .await
        .data()
        .clone()
    };
    let some = read(&dense).await;
    // The receipt states the counts once; no warning restates them.
    assert_eq!(
        (
            &some["selection"]["selectedItems"],
            &some["selection"]["totalItems"]
        ),
        (&json!(3), &json!(6)),
        "{some}"
    );
    assert!(some.get("warnings").is_none(), "{some}");
    let empty = read(&none).await;
    let warnings = empty["warnings"].to_string();
    assert!(
        warnings.contains("classification passed 0 of 2 hits")
            && warnings.contains("not absence")
            && empty["next"]["unfiltered"].is_object(),
        "{empty}"
    );
    runtime.close().await;
}

/// A whole-file read whose chunks all fail the classifiers is empty, and
/// says that an empty classified result is no absence.
#[tokio::test]
async fn a_classified_read_keeping_no_chunk_is_empty_and_not_absence() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("none.txt", "needle drop one\n\nneedle drop two\n");
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let row = support::run(
        &runtime,
        "localFetch",
        json!({"path":file,"classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = row.data();
    assert_eq!(row.status(), "empty", "{data}");
    assert_eq!(data["selection"]["unit"], "chunks", "{data}");
    assert_eq!(data["selection"]["selectedItems"], 0, "{data}");
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("classification passed 0 of")
            && warnings.contains("chunk")
            && warnings.contains("not absence"),
        "{data}"
    );
    assert!(data["next"]["unfiltered"].is_object(), "{data}");
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
    let mut kept = Vec::new();
    let mut decided;
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
        // Every candidate page counts the whole walk so far.
        let receipt = &data["selection"];
        decided =
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

mod github;
mod provider_faults;
