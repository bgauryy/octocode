#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::support;

use octocode_native::runtime::FailureKind;
use serde_json::{Value, json};
use support::{LocateTopPassage, Workspace, provider_runtime};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A provider stub answering `expected_calls` requests. B4: a bare (never
/// pooled) server: the process-wide judgment cache keys on the endpoint, so
/// a pooled server reused by another test that judged the same evidence
/// replayed its answer, the provider saw no call, and `expect` failed. The
/// questions here are worded apart from `runtime_clasify`'s over the same
/// `steps` source, so even a reused OS port cannot share a judgment.
async fn provider(expected_calls: u64) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateTopPassage {
            top: 0.9,
            exists: 0.9,
        })
        .expect(expected_calls)
        .mount(&server)
        .await;
    server
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

/// A locate target whose only content is one identifier or quoted literal
/// (bare, or in a lookup sentence) is a literal lookup: the call routes to an
/// executable `hints.textSearch` without reading the file or asking the
/// provider, and it is not a failure.
#[tokio::test]
async fn literal_locate_targets_route_to_local_search_without_provider_work() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = provider_runtime(&workspace, &server);
    for (ask, literal) in [
        ("step_17", "step_17"),
        ("Where is the step_17 function defined?", "step_17"),
        ("find `step_17()`", "step_17"),
        ("Where is \"fn step_17()\" defined", "fn step_17()"),
    ] {
        let input = json!({"queries":[{
            "reasoning":"Find the constant.","mainGoal":"Where step_17 is defined.",
            "resources":[{"id":"steps","tool":"localFetch","query":{
                "path":file,"fullContent":true
            }}],
            "questions":[{"id":"def","type":"locate","ask":ask}]
        }]});
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
        let hint = query["hints"]["text"][0].as_str().unwrap_or_default();
        assert!(
            hint.contains(literal) && hint.contains("localSearch"),
            "{query}"
        );
        let search = &query["hints"]["textSearch"];
        assert_eq!(search["tool"], "localSearch", "{query}");
        assert_eq!(
            search["query"]["queries"][0]["matchString"], literal,
            "{query}"
        );
        // A cross-tool lead starts its own step: it carries no matrix brief.
        assert!(
            search["query"]["queries"][0].get("mainGoal").is_none(),
            "{query}"
        );
        let found = runtime
            .execute(
                "literal-search".into(),
                "localSearch".into(),
                search["query"].clone(),
            )
            .await
            .expect("hints.textSearch executes");
        assert!(
            found
                .structured_content
                .to_string()
                .contains("fn step_17()"),
            "{ask}"
        );
    }
    runtime.close().await;
}

/// A matrix whose every question is a literal target, with different
/// literals, also routes without provider work: one alternation search
/// reaches every literal, and each literal gets its tip.
#[tokio::test]
async fn several_literal_targets_route_to_one_search_that_reaches_each() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({"queries":[{
        "resources":[{"id":"steps","tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}],
        "questions":[
            {"id":"a","type":"locate","ask":"Where is step_17 defined?"},
            {"id":"b","type":"locate","ask":"find step_23"}
        ]
    }]});
    let outcome = runtime
        .execute("locate-literals".into(), "clasify".into(), input)
        .await
        .expect("clasify");
    let output = &outcome.structured_content;
    octocode_native::contracts::validate_output("clasify", output).expect("output contract");
    let query = &output["queries"][0];
    assert_eq!(query["resources"], json!([]), "nothing was read: {query}");
    let tips = query["hints"]["text"].to_string();
    assert!(
        tips.contains("step_17") && tips.contains("step_23"),
        "{query}"
    );
    let search = &query["hints"]["textSearch"];
    assert_eq!(
        search["query"]["queries"][0]["matchString"], "step_17|step_23",
        "{query}"
    );
    let found = runtime
        .execute(
            "literal-search".into(),
            "localSearch".into(),
            search["query"].clone(),
        )
        .await
        .expect("hints.textSearch executes")
        .structured_content
        .to_string();
    assert!(
        found.contains("fn step_17()") && found.contains("fn step_23()"),
        "{found}"
    );
    runtime.close().await;
}

/// A described target that only mentions an identifier still runs locate,
/// and it gets no literal-search hint: the identifier is context.
#[tokio::test]
async fn a_described_target_that_names_an_identifier_is_still_located() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({"queries":[{
        "reasoning":"Locate a step.","mainGoal":"Where step_17 is defined.",
        "resources":[{"id":"steps","tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}],
        "questions":[{"id":"def","type":"locate","ask":"Where does step_17 return its value?"}]
    }]});
    let outcome = runtime
        .execute("locate-described".into(), "clasify".into(), input)
        .await
        .expect("clasify");
    let query = &outcome.structured_content["queries"][0];
    // Complete coverage is the default: no `coverage` field.
    assert!(query["resources"][0].get("coverage").is_none(), "{query}");
    assert!(query["best"]["def"][0]["line"].is_u64(), "{query}");
    assert!(query["hints"].get("text").is_none(), "{query}");
    assert!(query["hints"].get("textSearch").is_none(), "{query}");
    runtime.close().await;
}

/// On an open walk, `best` ranks only the pages judged so far, so a
/// confident window can still be a near miss: the response keeps every row
/// and says so in one tip. The finished walk carries no such tip.
#[tokio::test]
async fn an_open_walk_says_its_best_ranks_only_the_pages_judged_so_far() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateTopPassage {
            top: 0.9,
            exists: 0.9,
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let source = (1..=4000)
        .map(|n| format!("fn step_{n}() -> u32 {{\n    {n}\n}}\n"))
        .collect::<String>();
    let file = workspace
        .write("src/many_steps.rs", source)
        .to_string_lossy()
        .into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let mut request = Some(json!({"queries":[{
        "reasoning":"Locate a step.","mainGoal":"Which function returns one.",
        "resources":[{"id":"steps","tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}],
        "questions":[{"id":"t","type":"locate","ask":"The step function that returns one."}]
    }]}));
    let mut calls = 0;
    let mut open_tips = 0;
    while let Some(input) = request.take() {
        calls += 1;
        assert!(calls <= 10, "the walk terminates");
        let outcome = runtime
            .execute("locate-walk".into(), "clasify".into(), input)
            .await
            .expect("clasify");
        let output = &outcome.structured_content;
        octocode_native::contracts::validate_output("clasify", output).expect("output contract");
        let query = &output["queries"][0];
        let tip = query["hints"]["text"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|text| text.contains("pages judged so far") && text.contains("next.clasify"));
        request = query.pointer("/next/clasify").cloned();
        if request.is_some() {
            assert!(
                query["best"]["t"][0]["line"].is_u64(),
                "best stays: {query}"
            );
            assert_eq!(query["resources"][0]["coverage"], "partial", "{query}");
            assert!(tip, "open walk: {query}");
            open_tips += 1;
        } else {
            assert!(!tip, "finished walk: {query}");
        }
    }
    assert!(open_tips >= 1, "the file spans more than one call");
    runtime.close().await;
}

/// `best` already carries the read of its window; a page never repeats it.
#[tokio::test]
async fn a_page_read_never_repeats_a_best_row_read() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let file = steps(&workspace);
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({"queries":[{
        "reasoning":"Locate a step.","mainGoal":"Which function returns one.",
        "resources":[{"id":"steps","tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}],
        "questions":[{"id":"t","type":"locate","ask":"The step function that returns one."}]
    }]});
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
    let row = &query["best"]["t"][0];
    assert_eq!(
        best_read["query"]["queries"][0]["ranges"],
        json!([format!("{}-{}", row["line"], row["endLine"])]),
        "{query}"
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
    let server = MockServer::builder().start().await;
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
    let runtime = provider_runtime(&workspace, &server);
    let direct = runtime
        .execute(
            "read-missing".into(),
            "localFetch".into(),
            json!({"queries":[{"mainGoal":"g","reasoning":"r","path":missing}]}),
        )
        .await
        .expect("localFetch");
    assert_eq!(direct.failure, Some(FailureKind::NotFound));
    let matrix = |resources: Value| {
        json!({"queries":[{
            "reasoning":"Triage files.","mainGoal":"Which files define steps.",
            "resources":resources,
            "questions":[{"id":"q","type":"yesno","ask":"Does it define a step?"}]
        }]})
    };
    let resource = |id: &str, path: &str| json!({"id":id,"tool":"localFetch","query":{"path":path,"fullContent":true}});
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
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({"queries":[{
        "reasoning":"Rank dependents.","mainGoal":"Which modules import steps.",
        "resources":[{"id":"deps","tool":"astTopology","query":{
            "operation":"dependents","source":file
        }}],
        "questions":[{"id":"t","type":"yesno","ask":"Is this relevant?"}]
    }]});
    let error = runtime
        .execute("disabled-context".into(), "clasify".into(), input)
        .await
        .expect_err("a disabled context tool fails validation");
    assert_eq!(error.code, "invalidInput", "{error:?}");
    let issues = error.validation_issues.clone().unwrap_or_default();
    let issue = issues
        .iter()
        .find(|issue| issue.path.join(".") == "queries.0.resources.0.tool")
        .unwrap_or_else(|| panic!("{error:?}"));
    assert!(issue.message.contains("OCTOCODE_BETA"), "{}", issue.message);
    assert!(issue.message.contains("localFetch"), "{}", issue.message);
    assert!(!issue.message.contains("astRewrite"), "{}", issue.message);
    runtime.close().await;
}
