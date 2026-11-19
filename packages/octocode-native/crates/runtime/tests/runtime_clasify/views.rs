//! The two views of a clasify row. `summary` (the default) is each answer's
//! value, the deciding lines, the coverage, the tokens and the ready calls;
//! `details` is the whole row. A summary only removes: every fact it states
//! is in the details row of the same request.

use super::{Typed, clasify};
use crate::support::Run as _;
use crate::support::{Workspace, clasify_runtime};
use octocode_test_support::json_fixture;
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
    json_fixture(
        r#"{
            "restart": {"type": "noul", "instructions": "Does it restart?"},
            "kind": {
                "type": "choice",
                "criteria": {
                    "ops": "operations text",
                    "code": "source code",
                    "unknown": "cannot tell"
                }
            },
            "risk": {"type": "score", "criteria": ["low", "medium", "high"]}
        }"#,
    )
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
/// request ceiling, within the owner's 250 characters; details adds the
/// distribution, the legend, the model, the tokens and the request count.
#[tokio::test]
async fn a_text_answer_is_its_values_and_ceiling_by_default() {
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
    let (short, whole) = (
        &summary["results"][0]["data"],
        &details["results"][0]["data"],
    );
    assert_eq!(
        short,
        &json_fixture(
            r#"{
                "answers": {
                    "restart": {"noul": 0.9},
                    "kind": {"choice": "ops"},
                    "risk": {"score": 1.5}
                },
                "usage": {"requestsCeiling": 1}
            }"#
        ),
        "{summary}"
    );
    assert!(
        summary.to_string().len() <= 250,
        "{}",
        summary.to_string().len()
    );
    assert!(within(short, whole), "{summary}\n{details}");
    // The whole row: what a row returned before the views existed.
    assert_eq!(whole["model"], "jev-1.13.0", "{details}");
    assert_eq!(whole["usage"]["calls"], 1, "{details}");
    assert_eq!(whole["answers"]["kind"]["confidence"], 1, "{details}");
    assert_eq!(
        whole["answers"]["kind"]["probabilities"],
        json_fixture(r#"{"ops": 1.0, "code": 0.0, "unknown": 0.0}"#),
        "{details}"
    );
    assert_eq!(
        whole["answers"]["risk"]["legend"],
        json_fixture(r#"{"0": "low", "1": "medium", "2": "high"}"#),
        "{details}"
    );
    runtime.close().await;
}

/// Both views retain the captured source read without optional sections advice.
#[tokio::test]
async fn source_views_keep_required_reads_without_optional_sections_advice() {
    let server = provider().await;
    let workspace = Workspace::new();
    workspace.write("tiny.rs", "fn a() {} fn b() { planted() } fn c() {}\n");
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = |view: &str| {
        json!({"operation":"evaluate","view":view,
            "source":{"tool":"localFetch","query":{"path":"tiny.rs"}},
            "questions":{"p":{"type":"noul","instructions":"Is it planted?"}}})
    };
    let summary = clasify(&runtime, vec![row("summary")]).await;
    let details = clasify(&runtime, vec![row("details")]).await;
    let (short, whole) = (
        &summary["results"][0]["data"],
        &details["results"][0]["data"],
    );
    assert_eq!(
        short["answers"],
        json_fixture(r#"{"p": {"noul": 0.9}}"#),
        "{summary}"
    );
    assert_eq!(short["readChars"], whole["readChars"], "{summary}");
    assert!(short["next"]["read"].is_object(), "{summary}");
    assert!(short.get("related").is_none(), "{summary}");
    assert!(whole["next"]["read"].is_object(), "{details}");
    assert!(whole.get("related").is_none(), "{details}");
    runtime.close().await;
}

/// A sections page: a summary section is its lines and its answers, at most
/// 40% of the details section; the resource keeps its coverage and read.
#[tokio::test]
async fn a_section_summary_is_its_lines_and_answers() {
    let server = provider().await;
    let workspace = Workspace::new();
    let source: String = (1..=300)
        .map(|at| {
            format!("pub fn f_{at}(x: u32) -> u32 {{\n    let y = x + {at};\n    y * 2\n}}\n\n")
        })
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
        assert_eq!(
            section["target"],
            json!({"lines":full["target"]["lines"]}),
            "{summary}"
        );
        assert_eq!(
            section["answers"]["p"],
            json_fixture(r#"{"noul": 0.9}"#),
            "{summary}"
        );
        assert_eq!(
            section["answers"]["kind"],
            json_fixture(r#"{"choice": "ops"}"#),
            "{summary}"
        );
        assert!(within(section, full), "{section}\n{full}");
        let (small, large) = (section.to_string().len(), full.to_string().len());
        assert!(small * 100 <= large * 40, "{small} of {large}");
    }
    assert_eq!(short["coverage"], whole["coverage"], "{summary}");
    assert_eq!(short["sourceDigest"], whole["sourceDigest"], "{summary}");
    // The page's spend and its ceiling; its first round equals its calls.
    let usage = &summary["results"][0]["data"]["usage"];
    assert_eq!(
        usage.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["calls", "requestsCeiling"],
        "{summary}"
    );
    assert!(
        summary["results"][0]["data"].get("model").is_none(),
        "{summary}"
    );
    runtime.close().await;
}

/// A kept sections page leads to its own details: `next.details` replays the
/// page in view details without a provider request, and the details row
/// offers no such call again.
#[tokio::test]
async fn a_section_summary_replays_its_details_unbilled() {
    let server = provider().await;
    let workspace = Workspace::new();
    let source: String = (1..=120)
        .map(|at| format!("pub fn f_{at}(x: u32) -> u32 {{\n    x + {at}\n}}\n\n"))
        .collect();
    workspace.write("mid.rs", &source);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let summary = clasify(
        &runtime,
        vec![json_fixture(
            r#"{
                "operation": "sections",
                "view": "summary",
                "resources": {"mid": {"tool": "localFetch", "query": {"path": "mid.rs"}}},
                "questions": {
                    "kind": {
                        "type": "choice",
                        "criteria": {"ops": "operations text", "code": "source code"}
                    }
                }
            }"#,
        )],
    )
    .await;
    let data = &summary["results"][0]["data"];
    let call = &data["next"]["details"];
    assert_eq!(call["tool"], "clasify", "{summary}");
    let sent = server.received_requests().await.unwrap().len();
    let details = runtime
        .run(
            "clasify-test".into(),
            "clasify".into(),
            call["input"].clone(),
        )
        .await
        .expect("clasify result")
        .structured_content
        .into_value();
    let replay = &details["results"][0]["data"];
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        sent,
        "no provider request"
    );
    assert_eq!(replay["replayed"], true, "{details}");
    // The replay billed nothing, yet reports what the page cost, marked.
    assert_eq!(replay["usage"]["replayed"], true, "{details}");
    assert!(
        replay["usage"]["inputTokens"].as_u64() > Some(0),
        "{details}"
    );
    assert!(replay["usage"]["calls"].as_u64() > Some(0), "{details}");
    let sections = replay["resources"]["mid"]["sections"]
        .as_array()
        .expect("sections");
    assert!(
        sections.iter().all(|section| section.get("id").is_some()),
        "{details}"
    );
    assert!(
        sections
            .iter()
            .all(|section| section["answers"]["kind"]["answer"]["probabilities"].is_object()),
        "{details}"
    );
    assert!(replay["next"].get("details").is_none(), "{details}");
    runtime.close().await;
}

/// An absent `view` is a summary on CLI and MCP: a sections row sent without
/// it returns the summary shape and offers `next.details`, exactly as
/// `view:"summary"` does, and an evaluate row drops the distribution. Runs
/// the engine directly: the suite's `Run` asks for `details` when a row
/// states no view.
#[tokio::test]
async fn an_absent_view_is_a_summary_on_cli_and_mcp() {
    use octocode_native::runtime::Channel;
    let server = provider().await;
    let workspace = Workspace::new();
    let source: String = (1..=120)
        .map(|at| format!("pub fn f_{at}(x: u32) -> u32 {{\n    x + {at}\n}}\n\n"))
        .collect();
    workspace.write("mid.rs", &source);
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let sections = json_fixture(
        r#"{
            "operation": "sections",
            "resources": {"mid": {"tool": "localFetch", "query": {"path": "mid.rs"}}},
            "questions": {
                "kind": {
                    "type": "choice",
                    "criteria": {"ops": "operations text", "code": "source code"}
                }
            }
        }"#,
    );
    let evaluate =
        json!({"operation":"evaluate","content":"Rotate and restart.","questions":questions()});
    for channel in [Channel::Cli, Channel::Mcp] {
        let run = |row: Value| {
            let runtime = &runtime;
            async move {
                let admission = runtime.admit("absent-view".into()).expect("admitted");
                runtime
                    .execute(
                        admission,
                        "clasify".into(),
                        json!({"queries":[row]}),
                        channel,
                    )
                    .await
                    .expect("clasify result")
                    .structured_content
                    .into_value()
            }
        };
        let absent = run(sections.clone()).await;
        let data = &absent["results"][0]["data"];
        assert_eq!(data["next"]["details"]["tool"], "clasify", "{absent}");
        let shown = data["resources"]["mid"]["sections"]
            .as_array()
            .expect("sections");
        assert!(
            shown.iter().all(|section| section.get("id").is_none()
                && section["answers"]["kind"].get("status").is_none()),
            "{absent}"
        );
        let mut summary = sections.clone();
        summary["view"] = json!("summary");
        let explicit = run(summary).await;
        assert_eq!(
            explicit["results"][0]["data"]["resources"], data["resources"],
            "{explicit}"
        );
        let answered = run(evaluate.clone()).await;
        assert!(
            answered["results"][0]["data"]["answers"]["kind"]
                .get("probabilities")
                .is_none(),
            "{answered}"
        );
    }
    runtime.close().await;
}

/// Two identical rows share one request: the first row paid, the second
/// shows `calls: 0`, and the summary states `calls` on both so a reader sees
/// who paid. A single row states no usage.
#[tokio::test]
async fn a_summary_states_calls_on_every_row_when_a_sibling_shows_zero_and_a_lone_row_its_ceiling()
{
    let server = provider().await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let row = json_fixture(
        r#"{
            "operation": "evaluate",
            "view": "summary",
            "content": "The service rotates credentials nightly.",
            "questions": {
                "restart": {"type": "noul", "instructions": "Does it restart?"}
            }
        }"#,
    );
    let out = clasify(&runtime, vec![row.clone(), row.clone()]).await;
    assert_eq!(out["results"][0]["data"]["usage"]["calls"], 1, "{out}");
    assert_eq!(out["results"][1]["data"]["usage"]["calls"], 0, "{out}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1, "{out}");
    let single = clasify(
        &runtime,
        vec![json_fixture(
            r#"{
                "operation": "evaluate",
                "view": "summary",
                "content": "A different text, billed alone.",
                "questions": {
                    "restart": {"type": "noul", "instructions": "Does it restart?"}
                }
            }"#,
        )],
    )
    .await;
    assert_eq!(
        single["results"][0]["data"]["usage"],
        json!({"requestsCeiling": 1}),
        "{single}"
    );
    runtime.close().await;
}
