//! Facts across the rows of one call (C3eF R1, R6): the sections lead is
//! offered once, only for a noul yes, over every source that said yes; and a
//! question every answer of which is unsure says so once.

use super::{clasify, noul, sent};
use crate::support::{Workspace, clasify_runtime};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Answers a noul question with the number a source states as `NOUL=<x>`
/// (0.9 when it states none) and a choice with its first label.
struct Stated;

impl Respond for Stated {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).expect("provider body");
        let text = body["state"].to_string();
        let stated = text
            .split_once("NOUL=")
            .and_then(|(_, rest)| rest.get(..4))
            .and_then(|number| number.parse::<f64>().ok())
            .unwrap_or(0.9);
        let answers = body["questions"]
            .as_object()
            .expect("questions")
            .iter()
            .map(|(id, question)| {
                let answer = if question["type"] == "choice" {
                    let labels = question["criteria"].as_object().expect("labels");
                    let first = labels.keys().next().expect("a label").clone();
                    let probabilities = labels
                        .keys()
                        .map(|label| (label.clone(), json!(f64::from(u8::from(*label == first)))))
                        .collect::<serde_json::Map<_, _>>();
                    json!({"type":"choice","choice":first,"confidence":1,"probabilities":probabilities})
                } else {
                    json!({"type":"noul","noul":stated})
                };
                (id.clone(), answer)
            })
            .collect::<serde_json::Map<_, _>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":40,"output_tokens":7}
        }))
    }
}

async fn stated_provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(Stated)
        .mount(&server)
        .await;
    server
}

fn source(path: &str) -> Value {
    json!({"tool":"localFetch","query":{"path":path}})
}

fn choice(instructions: &str) -> Value {
    json!({"type":"choice","instructions":instructions,
        "criteria":{"config":"A configuration file.","code":"Source code."}})
}

/// Every `clasify` related call in the response, with the row it is on.
fn sections_leads(out: &Value) -> Vec<(usize, Value)> {
    out["results"]
        .as_array()
        .expect("results")
        .iter()
        .enumerate()
        .flat_map(|(index, row)| {
            row["data"]["related"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|lead| lead["tool"] == "clasify")
                .map(move |lead| (index, lead["input"]["queries"][0].clone()))
        })
        .collect()
}

/// R1: a choice answer is no yes, so it offers no sections lead.
#[tokio::test]
async fn choice_rows_offer_no_sections_lead() {
    let server = stated_provider().await;
    let workspace = Workspace::new();
    workspace.write("a.toml", "name = \"a\"\n");
    workspace.write("b.toml", "name = \"b\"\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |file: &str| {
        json!({"operation":"evaluate","source":source(file),
            "questions":{"kind":choice("What kind of file is this?")}})
    };
    let out = clasify(&runtime, vec![row("a.toml"), row("b.toml")]).await;
    let data = &out["results"][0]["data"];
    eprintln!(
        "R1 evaluate-row chars (choice row over a.toml): {}",
        data.to_string().chars().count()
    );
    assert_eq!(data["answers"]["kind"]["choice"], "config", "{out:#}");
    assert!(sections_leads(&out).is_empty(), "{out:#}");
    runtime.close().await;
}

/// R1: rows whose yes answers share questions, context and model offer one
/// sections call naming every yes source, on the first of them, with only
/// the questions that said yes.
#[tokio::test]
async fn yes_rows_with_one_question_share_one_sections_call() {
    let server = stated_provider().await;
    let workspace = Workspace::new();
    workspace.write("one.rs", "fn one() {} // restart\n");
    workspace.write("two.rs", "fn two() {} // restart\n");
    workspace.write("three.rs", "fn three() {} // NOUL=0.10\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |file: &str| {
        json!({"operation":"evaluate","source":source(file),
            "questions":{"restart":noul("Does it restart?"),"kind":choice("What kind of file is this?")}})
    };
    let out = clasify(
        &runtime,
        vec![row("one.rs"), row("two.rs"), row("three.rs")],
    )
    .await;
    assert_eq!(sent(&server).await.len(), 3, "one request per row");
    let leads = sections_leads(&out);
    assert_eq!(leads.len(), 1, "one sections call in the response: {out:#}");
    let (at, call) = &leads[0];
    assert_eq!(*at, 0, "on the first yes row: {out:#}");
    assert_eq!(call["operation"], "sections");
    assert_eq!(
        call["questions"],
        json!({"restart":noul("Does it restart?")}),
        "only the yes questions: {call:#}"
    );
    let named = call["resources"]
        .as_object()
        .expect("resources")
        .values()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(named, vec![source("one.rs"), source("two.rs")], "{call:#}");
    runtime.close().await;
}

/// R6: three judgments of one question, every one unsure, carry one hint
/// that names the question and the range; one sure answer clears it.
#[tokio::test]
async fn a_question_every_answer_of_which_is_unsure_says_so_once() {
    let server = stated_provider().await;
    let workspace = Workspace::new();
    for (file, noul) in [("a.md", "0.51"), ("b.md", "0.64"), ("c.md", "0.55")] {
        workspace.write(file, format!("NOUL={noul}\n"));
    }
    workspace.write("d.md", "NOUL=0.95\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |file: &str| {
        json!({"operation":"evaluate","source":source(file),
            "questions":{"fit":noul("Is it good?")}})
    };
    let hint = "Every answer to `fit` is unsure (0.51–0.64): the question may be vague; ask one observable fact, or a choice with named options.";
    let out = clasify(&runtime, vec![row("a.md"), row("b.md"), row("c.md")]).await;
    let carried = out["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|row| {
            row["data"]["hints"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|text| *text == hint)
                .count()
        })
        .collect::<Vec<_>>();
    assert_eq!(carried, vec![1, 0, 0], "{out:#}");
    let sure = clasify(
        &runtime,
        vec![row("a.md"), row("b.md"), row("c.md"), row("d.md")],
    )
    .await;
    assert!(!sure.to_string().contains("is unsure"), "{sure:#}");
    runtime.close().await;
}
