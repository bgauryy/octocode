//! The two views of a clasify row. `summary` (the default) is each answer's
//! value, the deciding lines, the coverage, the tokens and the ready calls;
//! `details` is the whole row. A summary only removes: every fact it states
//! is in the details row of the same request.

use super::{Typed, clasify};
use crate::support::{Workspace, clasify_runtime};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

async fn provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(Typed)
        .mount(&server)
        .await;
    server
}

fn questions() -> Value {
    json!({
        "restart":{"type":"noul","instructions":"Does it restart?"},
        "kind":{"type":"choice","criteria":{"ops":"operations text","code":"source code","unknown":"cannot tell"}},
        "risk":{"type":"score","criteria":["low","medium","high"]},
    })
}

/// Whether every fact `summary` states is in `details`: the same leaf at the
/// same place, or an answered cell's answer under its status wrapper.
fn within(summary: &Value, details: &Value) -> bool {
    match (summary, details) {
        (Value::Object(fields), Value::Object(whole)) => {
            if whole.get("status") == Some(&json!("answered")) && !fields.contains_key("status") {
                return within(summary, &whole["answer"]);
            }
            fields
                .iter()
                .all(|(key, value)| whole.get(key).is_some_and(|other| within(value, other)))
        }
        (Value::Array(items), Value::Array(whole)) => items
            .iter()
            .all(|item| whole.iter().any(|other| within(item, other))),
        _ => summary == details,
    }
}

/// Three questions over a text: the summary is the three values and the
/// tokens, within the owner's 250 characters; details adds the distribution,
/// the legend, the model and the request count.
#[tokio::test]
async fn a_text_answer_is_its_values_and_tokens_by_default() {
    let server = provider().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |view: &str| {
        json!({"operation":"evaluate","view":view,
            "content":"The service rotates credentials nightly and restarts after each rotation.",
            "questions":questions()})
    };
    let summary = clasify(&runtime, vec![row("summary")]).await;
    let details = clasify(&runtime, vec![row("details")]).await;
    let (short, whole) = (&summary["results"][0]["data"], &details["results"][0]["data"]);
    assert_eq!(
        short,
        &json!({
            "answers":{"restart":{"noul":0.9},"kind":{"choice":"ops"},"risk":{"score":1.5}},
            "usage":{"inputTokens":40,"outputTokens":7},
        }),
        "{summary}"
    );
    assert!(summary.to_string().len() <= 250, "{}", summary.to_string().len());
    assert!(within(short, whole), "{summary}\n{details}");
    // The whole row: what a row returned before the views existed.
    assert_eq!(whole["model"], "jev-1.13.0", "{details}");
    assert_eq!(whole["usage"]["calls"], 1, "{details}");
    assert_eq!(whole["answers"]["kind"]["confidence"], 1, "{details}");
    assert_eq!(
        whole["answers"]["kind"]["probabilities"],
        json!({"ops":1.0,"code":0.0,"unknown":0.0}),
        "{details}"
    );
    assert_eq!(
        whole["answers"]["risk"]["legend"],
        json!({"0":"low","1":"medium","2":"high"}),
        "{details}"
    );
    runtime.close().await;
}

/// A source row keeps what the next call needs: how much was judged and the
/// read of it. On a source shorter than a `sections` lead, that lead goes.
#[tokio::test]
async fn a_source_summary_keeps_its_read_and_drops_a_lead_longer_than_the_source() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("tiny.rs", "fn a() {}\nfn b() { planted() }\nfn c() {}\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |view: &str| {
        json!({"operation":"evaluate","view":view,
            "source":{"tool":"localFetch","query":{"path":"tiny.rs"}},
            "questions":{"p":{"type":"noul","instructions":"Is it planted?"}}})
    };
    let summary = clasify(&runtime, vec![row("summary")]).await;
    let details = clasify(&runtime, vec![row("details")]).await;
    let (short, whole) = (&summary["results"][0]["data"], &details["results"][0]["data"]);
    assert_eq!(short["answers"], json!({"p":{"noul":0.9}}), "{summary}");
    assert_eq!(short["readChars"], whole["readChars"], "{summary}");
    assert!(short["next"]["read"].is_object(), "{summary}");
    assert!(short.get("related").is_none(), "{summary}");
    let lead = whole["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "clasify");
    assert!(lead.is_some(), "details keeps the sections lead: {details}");
    runtime.close().await;
}

/// A sections page: a summary section is its lines and its answers, at most
/// 40% of the details section; the resource keeps its coverage and read.
#[tokio::test]
async fn a_section_summary_is_its_lines_and_answers() {
    let server = provider().await;
    let workspace = Workspace::new();
    let source: String = (1..=300)
        .map(|at| format!("pub fn f_{at}(x: u32) -> u32 {{\n    let y = x + {at};\n    y * 2\n}}\n\n"))
        .collect();
    workspace.write("mid.rs", &source);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |view: &str| {
        json!({"operation":"sections","view":view,
            "resources":{"mid":{"tool":"localFetch","query":{"path":"mid.rs"}}},
            "questions":{
                "p":{"type":"noul","instructions":"Is it planted?"},
                "kind":{"type":"choice","criteria":{"ops":"operations text","code":"source code"}},
            }})
    };
    let summary = clasify(&runtime, vec![row("summary")]).await;
    let details = clasify(&runtime, vec![row("details")]).await;
    let (short, whole) = (
        &summary["results"][0]["data"]["resources"]["mid"],
        &details["results"][0]["data"]["resources"]["mid"],
    );
    let (short_sections, whole_sections) = (
        short["sections"].as_array().expect("sections"),
        whole["sections"].as_array().expect("sections"),
    );
    assert!(!short_sections.is_empty(), "{summary}");
    assert_eq!(short_sections.len(), whole_sections.len(), "{summary}");
    for (section, full) in short_sections.iter().zip(whole_sections) {
        assert_eq!(
            section.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["target", "answers"],
            "{summary}"
        );
        assert_eq!(section["target"], json!({"lines":full["target"]["lines"]}), "{summary}");
        assert_eq!(section["answers"]["p"], json!({"noul":0.9}), "{summary}");
        assert_eq!(section["answers"]["kind"], json!({"choice":"ops"}), "{summary}");
        assert!(within(section, full), "{section}\n{full}");
        let (small, large) = (section.to_string().len(), full.to_string().len());
        assert!(small * 100 <= large * 40, "{small} of {large}");
    }
    assert_eq!(short["coverage"], whole["coverage"], "{summary}");
    assert_eq!(short["snapshot"], whole["snapshot"], "{summary}");
    let usage = &summary["results"][0]["data"]["usage"];
    assert_eq!(
        usage.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["calls", "inputTokens", "outputTokens"],
        "{summary}"
    );
    assert!(summary["results"][0]["data"].get("model").is_none(), "{summary}");
    runtime.close().await;
}
