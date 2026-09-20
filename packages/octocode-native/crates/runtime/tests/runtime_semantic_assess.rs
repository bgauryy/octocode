#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use serde_json::json;
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn query() -> serde_json::Value {
    json!({
        "id":"decision",
        "reasoning":"Choose the next inspection.",
        "resources":[{"id":"observed","context":{"value":{"fact":"present"}}}],
        "questions":[
            {"id":"relevant","question":{"type":"noul","instructions":"Is it relevant?"}},
            {"id":"risk","question":{"type":"score","instructions":{"prompt":"Rate risk"},"criteria":["low",{"label":"high"}]}}
        ]
    })
}

#[test]
fn cli_lists_semantic_assess_without_legacy_alias_and_explains_key_setup() {
    let workspace = Workspace::new();
    let help = workspace.cli().arg("--help").output().expect("CLI help");
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("\n  semanticAssess"));
    assert!(!help.contains("\n  jev"));

    let output = workspace
        .cli()
        .args(["semanticAssess", &query().to_string(), "--compact"])
        .output()
        .expect("missing-key execution");
    assert_eq!(output.status.code(), Some(5));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("missingConfiguration"), "{stdout}");
    assert!(stdout.contains("OCTOCODE_JEV_KEY"), "{stdout}");
    assert!(
        stdout.contains("https://docs.typesafe.ai/introduction"),
        "{stdout}"
    );
}

#[tokio::test]
async fn public_identity_is_a_hard_cutover_and_missing_key_is_actionable() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    assert!(!runtime.is_available("semanticAssess"));
    assert!(!runtime.is_available("jev"));
    let catalog = runtime.catalog().unwrap();
    assert!(
        catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| { tool["name"] == "semanticAssess" && tool["available"] == false })
    );
    assert!(
        !catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "jev")
    );

    let error = runtime
        .execute("missing-key".into(), "semanticAssess".into(), query())
        .await
        .unwrap_err();
    assert_eq!(error.code, "missingConfiguration");
    assert!(error.message.contains("OCTOCODE_JEV_KEY"));
    assert!(
        error
            .message
            .contains("https://docs.typesafe.ai/introduction")
    );
    runtime.close().await;
}

#[tokio::test]
async fn matrix_is_resource_major_and_reports_requested_and_resolved_models() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"provider-resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.9},
                "answer_1":{"type":"score","score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":"low","1":{"label":"high"}}}
            },
            "usage":{"input_tokens":12,"output_tokens":3}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".into()),
        ("OCTOCODE_JEV_MODEL", "caller-requested".into()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
    ]);
    let outcome = runtime
        .execute("matrix".into(), "semanticAssess".into(), query())
        .await
        .unwrap();
    octocode_native::contracts::validate_output("semanticAssess", &outcome.structured_content)
        .expect("nested output contract");
    let queries = outcome.structured_content["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["queryId"], "decision");
    let cells = queries[0]["results"].as_array().unwrap();
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0]["resourceId"], "observed");
    assert_eq!(cells[0]["questionId"], "relevant");
    assert_eq!(cells[1]["questionId"], "risk");
    for cell in cells {
        assert_eq!(
            cell["coverage"], "complete",
            "{}",
            outcome.structured_content
        );
        assert_eq!(cell["pages"].as_array().unwrap().len(), 1);
        assert_eq!(cell["pages"][0]["requestedModel"], "caller-requested");
        assert_eq!(cell["pages"][0]["resolvedModel"], "provider-resolved");
    }
    runtime.close().await;
}

#[tokio::test]
async fn oversized_first_page_is_bounded_partial_without_a_looping_continuation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".into()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
    ]);
    let input = json!({
        "id":"bounded",
        "reasoning":"Bound the supplied resource.",
        "resources":[{"id":"large","maxChars":5,"context":{"value":{"text":"far too large"}}}],
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute("bounded".into(), "semanticAssess".into(), input)
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert_eq!(query["results"][0]["coverage"], "partial");
    assert_eq!(
        query["results"][0]["pages"][0]["context"]["coverage"],
        "partial"
    );
    assert!(query.get("next").is_none());
    runtime.close().await;
}

#[tokio::test]
async fn recoverable_full_content_error_follows_exact_pages_and_completes_coverage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(4)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("large.txt", "x".repeat(60_000));
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".into()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
        ("REQUEST_TIMEOUT", "20000".into()),
    ]);
    let input = json!({
        "id":"recover-full-content",
        "reasoning":"Recover the exact pages of an oversized whole-file request.",
        "resources":[{"id":"file","maxChars":80_000,"context":{"tool":"localFetch","query":{
            "path":file,"reasoning":"Read the complete file.","fullContent":true
        }}}],
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute(
            "recover-full-content".into(),
            "semanticAssess".into(),
            input,
        )
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert!(query.get("next").is_none(), "{query}");
    let cell = &query["results"][0];
    assert_eq!(cell["coverage"], "complete", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 4, "{cell}");
    assert_eq!(pages[0]["status"], "success");
    assert_eq!(pages[0]["context"]["coverage"], "partial");
    assert_eq!(pages[3]["status"], "success");
    assert_eq!(pages[3]["context"]["coverage"], "bounded");
    octocode_native::contracts::validate_output("semanticAssess", &outcome.structured_content)
        .expect("recovered semantic output contract");
    runtime.close().await;
}

#[tokio::test]
async fn page_budget_continuation_round_trips_through_the_public_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "many-lines.txt",
        (0..200)
            .map(|index| format!("line {index}\n"))
            .collect::<String>(),
    );
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".into()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
    ]);
    let input = json!({
        "id":"paged",
        "reasoning":"Assess bounded pages.",
        "resources":[{"id":"file","maxChars":5000,"context":{"tool":"localFetch","query":{
            "path":file,"reasoning":"Read the next exact line.","limit":1,"fullContent":false
        }}}],
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute("paged".into(), "semanticAssess".into(), input)
        .await
        .unwrap();
    let assess = outcome.structured_content["queries"][0]["next"]["assess"].clone();
    assert!(assess.is_object(), "{}", outcome.structured_content);
    let context = &assess["resources"][0]["context"];
    assert_eq!(context.as_object().unwrap().len(), 2);
    assert!(context.get("tool").is_some());
    assert!(context.get("query").is_some());
    octocode_native::contracts::prepare_many_and_validate(
        "semanticAssess",
        assess,
        octocode_native::contracts::PrepareOptions::default(),
    )
    .expect("next.assess must execute unchanged");
    octocode_native::contracts::validate_output("semanticAssess", &outcome.structured_content)
        .expect("nested query-cell-page output");
    runtime.close().await;
}
