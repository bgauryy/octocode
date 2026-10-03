#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod support;

use octocode_native::runtime::FailureKind;
use serde_json::{Value, json};
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const MOCK_PROVIDER_TIMEOUT_MS: &str = "30000";

/// Locate stub: 0.9 on the first passage offered, `exists` 0.9.
#[derive(Clone)]
struct LocateFirstPassage;

impl Respond for LocateFirstPassage {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        fn passage_ids(value: &Value, ids: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    for (key, value) in map {
                        if key.len() == 4 && key.starts_with('P') && !ids.contains(key) {
                            ids.push(key.clone());
                        }
                        passage_ids(value, ids);
                    }
                }
                Value::Array(items) => items.iter().for_each(|item| passage_ids(item, ids)),
                _ => {}
            }
        }
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let mut ids = Vec::new();
        passage_ids(&body, &mut ids);
        ids.sort();
        let rest = if ids.len() > 1 {
            0.1 / (ids.len() - 1) as f64
        } else {
            0.0
        };
        let probabilities = ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), json!(if index == 0 { 0.9 } else { rest })))
            .collect::<serde_json::Map<_, _>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"choice","choice":ids.first(),"confidence":0.9,
                    "probabilities":probabilities},
                "answer_1":{"type":"noul","noul":0.9}
            },
            "usage":{"input_tokens":5,"output_tokens":2}
        }))
    }
}

async fn provider(expected_calls: u64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateFirstPassage)
        .expect(expected_calls)
        .mount(&server)
        .await;
    server
}

fn runtime(workspace: &Workspace, server: &MockServer) -> octocode_native::runtime::ToolRuntime {
    workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ])
}

fn steps(workspace: &Workspace) -> String {
    let source = (1..=40)
        .map(|n| format!("fn step_{n}() -> u32 {{\n    {n}\n}}\n"))
        .collect::<String>();
    workspace
        .write("src/steps.rs", source)
        .to_string_lossy()
        .into_owned()
}

/// A locate target that is only an identifier is a literal lookup: the call
/// routes to an executable `next.localSearch` without reading the file or
/// asking the provider, and it is not a failure.
#[tokio::test]
async fn bare_identifier_locate_target_routes_to_local_search_without_provider_work() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = runtime(&workspace, &server);
    let input = json!({
        "reasoning":"Find the constant.","mainGoal":"Where step_17 is defined.",
        "resources":[{"id":"steps","context":{"tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}}],
        "questions":[{"id":"def","questionType":"locate","target":"step_17"}]
    });
    let outcome = runtime
        .execute("locate-bare".into(), "clasify".into(), input)
        .await
        .expect("clasify");
    let output = &outcome.structured_content;
    octocode_native::contracts::validate_output("clasify", output).expect("output contract");
    assert!(!outcome.all_failed, "{output}");
    assert!(outcome.failure.is_none(), "{output}");
    let query = &output["queries"][0];
    assert_eq!(query["resources"], json!([]), "nothing was read: {query}");
    assert!(query.get("best").is_none(), "{query}");
    assert!(
        query["hints"]["text"][0]
            .as_str()
            .unwrap_or_default()
            .contains("step_17"),
        "{query}"
    );
    let search = &query["hints"]["localSearch"];
    assert_eq!(search["query"]["searchText"], "step_17", "{query}");
    assert_eq!(
        search["query"]["mainGoal"], "Where step_17 is defined.",
        "{query}"
    );
    assert_eq!(
        search["query"]["reasoning"], "Find the constant.",
        "{query}"
    );
    let found = runtime
        .execute(
            "literal-search".into(),
            "localSearch".into(),
            search["query"].clone(),
        )
        .await
        .expect("next.localSearch executes");
    assert!(
        found
            .structured_content
            .to_string()
            .contains("fn step_17()")
    );
    runtime.close().await;
}

/// A described target naming an identifier still runs locate (the hint
/// remains advisory), and a remote resource never short-circuits.
#[tokio::test]
async fn a_described_target_that_names_an_identifier_is_still_located() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = runtime(&workspace, &server);
    let input = json!({
        "reasoning":"Locate a step.","mainGoal":"Where step_17 is defined.",
        "resources":[{"id":"steps","context":{"tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}}],
        "questions":[{"id":"def","questionType":"locate","target":"Where is step_17 defined?"}]
    });
    let outcome = runtime
        .execute("locate-described".into(), "clasify".into(), input)
        .await
        .expect("clasify");
    let query = &outcome.structured_content["queries"][0];
    // Complete coverage is the default: no `coverage` field.
    assert!(query["resources"][0].get("coverage").is_none(), "{query}");
    assert!(query["best"]["def"][0]["lines"].is_array(), "{query}");
    runtime.close().await;
}

/// `best` already carries the read of its window; a page never repeats it.
#[tokio::test]
async fn a_page_read_never_repeats_a_best_row_read() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = runtime(&workspace, &server);
    let input = json!({
        "reasoning":"Locate a step.","mainGoal":"Which function returns one.",
        "resources":[{"id":"steps","context":{"tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}}],
        "questions":[{"id":"t","questionType":"locate","target":"The function that returns one."}]
    });
    let outcome = runtime
        .execute("locate-dedup".into(), "clasify".into(), input)
        .await
        .expect("clasify");
    let output = &outcome.structured_content;
    octocode_native::contracts::validate_output("clasify", output).expect("output contract");
    let query = &output["queries"][0];
    assert!(query["best"]["t"].is_array(), "best: {query}");
    // The top best window's read is the query's next.read, emitted once.
    let best_read = &query["hints"]["read"];
    assert!(best_read.is_object(), "{query}");
    assert_eq!(
        best_read["query"]["startLine"],
        query["best"]["t"][0]["lines"][0]
    );
    for page in query["resources"][0]["pages"].as_array().unwrap() {
        assert_ne!(
            page.pointer("/hints/read"),
            Some(best_read),
            "duplicate read: {query}"
        );
    }
    runtime.close().await;
}

/// A missing file is not-found, exactly as the same localFetch reports it;
/// a batch in which only some resources are missing is not a failure.
#[tokio::test]
async fn a_missing_file_resource_is_not_found_like_its_read() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let missing = workspace
        .workspace
        .join("src/nope.rs")
        .to_string_lossy()
        .into_owned();
    let runtime = runtime(&workspace, &server);
    let direct = runtime
        .execute(
            "read-missing".into(),
            "localFetch".into(),
            json!({"mainGoal":"g","reasoning":"r","path":missing}),
        )
        .await
        .expect("localFetch");
    assert_eq!(direct.failure, Some(FailureKind::NotFound));
    let matrix = |resources: Value| {
        json!({
            "reasoning":"Triage files.","mainGoal":"Which files define steps.",
            "resources":resources,
            "questions":[{"id":"q","type":"noul","instructions":"Does it define a step?"}]
        })
    };
    let resource = |id: &str, path: &str| json!({"id":id,"context":{"tool":"localFetch","query":{"path":path,"fullContent":true}}});
    let all_missing = runtime
        .execute(
            "clasify-missing".into(),
            "clasify".into(),
            matrix(json!([resource("x", &missing)])),
        )
        .await
        .expect("clasify");
    assert!(all_missing.all_failed);
    assert_eq!(
        all_missing.failure,
        Some(FailureKind::NotFound),
        "{}",
        all_missing.structured_content
    );
    let mixed = runtime
        .execute(
            "clasify-mixed".into(),
            "clasify".into(),
            matrix(json!([resource("ok", &file), resource("x", &missing)])),
        )
        .await
        .expect("clasify");
    assert!(!mixed.all_failed);
    assert!(mixed.failure.is_none(), "{}", mixed.structured_content);
    runtime.close().await;
}

/// A context tool this runtime does not enable (astTopology with beta off) is
/// rejected at validation, like MCP's availability-scoped schema, naming the
/// gate that enables it and the context tools that are enabled.
#[tokio::test]
async fn a_disabled_context_tool_is_rejected_at_validation_naming_its_gate() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = runtime(&workspace, &server);
    let input = json!({
        "reasoning":"Rank dependents.","mainGoal":"Which modules import steps.",
        "resources":[{"id":"deps","context":{"tool":"astTopology","query":{
            "analysis":"dependents","file":file
        }}}],
        "questions":[{"id":"t","type":"noul","instructions":"Is this relevant?"}]
    });
    let error = runtime
        .execute("disabled-context".into(), "clasify".into(), input)
        .await
        .expect_err("a disabled context tool fails validation");
    assert_eq!(error.code, "invalidInput", "{error:?}");
    let issues = error.validation_issues.clone().unwrap_or_default();
    let issue = issues
        .iter()
        .find(|issue| issue.path.join(".") == "resources.0.context.tool")
        .unwrap_or_else(|| panic!("{error:?}"));
    assert!(issue.message.contains("OCTOCODE_BETA"), "{}", issue.message);
    assert!(issue.message.contains("localFetch"), "{}", issue.message);
    assert!(!issue.message.contains("astRewrite"), "{}", issue.message);
    runtime.close().await;
}
