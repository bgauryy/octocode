//! State-row judgments: the provider exchange, models, replay and single flight, persistence, pages and the selection deadline.

use super::{clasify, noul, provider, sent, state_row};
use crate::support::{self, Run as _, Workspace, clasify_runtime, provider_env};
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

#[tokio::test]
async fn a_state_row_sends_the_questions_unchanged_and_returns_provider_answers() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut row = state_row();
    row["questions"] = json!({
        "restart": {"type":"noul","instructions":{"question":"Requires `action`?","action":"restart"},
            "criteria":{"true":{"meaning":"yes"},"false":["no"]}},
        "route": {"type":"choice","instructions":"Which procedure?",
            "criteria":{"restart":"Restart.","hot":["No restart."],"unknown":null}},
        "support": {"type":"score","instructions":"How direct?",
            "criteria":[{"meaning":"Unrelated."},["Mentions it."],"Steps."]}
    });
    let out = clasify(&runtime, vec![row.clone()]).await;
    let request = &sent(&server).await[0];
    assert_eq!(request["state"], row["state"]);
    assert_eq!(request["model"], "jev-latest", "the configured default");
    // The questions reach the provider exactly as asked: no brief, preset or
    // renaming. Correlation keys are internal.
    let mut asked = request["questions"]
        .as_object()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    asked.sort_by_key(Value::to_string);
    let mut expected = row["questions"]
        .as_object()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    expected.sort_by_key(Value::to_string);
    assert_eq!(asked, expected);
    assert!(!request.to_string().contains("Metadata only"));
    assert!(!request.to_string().contains("Decide whether rotation"));

    let result = &out["queries"][0];
    assert_eq!(result["id"], "rotation");
    assert_eq!(result["model"], "jev-1.13.0");
    assert_eq!(
        result["usage"],
        json!({"input_tokens":40,"output_tokens":7})
    );
    assert_eq!(
        result["answers"]["restart"],
        json!({"type":"noul","noul":0.9})
    );
    assert_eq!(result["answers"]["route"]["type"], "choice");
    assert_eq!(result["answers"]["route"]["choice"], "restart");
    assert_eq!(
        result["answers"]["route"]["probabilities"],
        json!({"restart":1.0,"hot":0.0,"unknown":0.0})
    );
    assert_eq!(
        result["answers"]["support"]["score"], 1.5,
        "fractional, never rounded"
    );
    assert_eq!(
        result["answers"]["support"]["legend"],
        json!({"0":{"meaning":"Unrelated."},"1":["Mentions it."],"2":"Steps."})
    );
    assert!(
        result.get("hints").is_none(),
        "a state row has no source to read"
    );
    assert!(result.get("replayed").is_none());
    runtime.close().await;
}

#[tokio::test]
async fn a_row_model_and_the_configured_model_reach_the_provider() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(
        &workspace,
        &server,
        &[("OCTOCODE_CLASSIFICATION_MODEL", "jev-1.13.0".into())],
    );
    let mut pinned = state_row();
    pinned["id"] = json!("pinned");
    pinned["model"] = json!("jev-1.14.0");
    clasify(&runtime, vec![state_row(), pinned]).await;
    let mut models = sent(&server)
        .await
        .iter()
        .map(|request| request["model"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    models.sort();
    assert_eq!(models, ["jev-1.13.0", "jev-1.14.0"]);
    runtime.close().await;
}

#[tokio::test]
async fn an_identical_evaluation_replays_without_a_provider_request() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let first = clasify(&runtime, vec![state_row()]).await;
    let again = clasify(&runtime, vec![state_row()]).await;
    assert!(first["queries"][0].get("replayed").is_none());
    assert_eq!(again["queries"][0]["replayed"], true);
    assert_eq!(
        again["queries"][0]["answers"],
        first["queries"][0]["answers"]
    );
    assert_eq!(again["queries"][0]["usage"], first["queries"][0]["usage"]);
    // A different question is a different evaluation.
    let mut other = state_row();
    other["questions"] = json!({"restart":noul("Does `document.text` forbid a restart?")});
    let changed = clasify(&runtime, vec![other]).await;
    assert!(changed["queries"][0].get("replayed").is_none());
    assert_eq!(sent(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn identical_rows_in_one_call_share_one_provider_request() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut second = state_row();
    second["id"] = json!("again");
    let out = clasify(&runtime, vec![state_row(), second]).await;
    let rows = out["queries"].as_array().unwrap();
    assert_eq!(rows[0]["answers"], rows[1]["answers"]);
    assert_eq!(
        rows.iter()
            .filter(|row| row.get("replayed").is_some())
            .count(),
        1,
        "{out}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn persistent_judgments_serve_a_fresh_runtime_and_memory_mode_does_not() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    let env = |mode: &str| [("OCTOCODE_STORAGE_MODE", mode.to_owned())];
    for (mode, replayed) in [
        ("persistent", None),
        ("persistent", Some(true)),
        ("memory", None),
        ("memory", None),
    ] {
        let runtime = clasify_runtime(&workspace, &server, &env(mode));
        let mut row = state_row();
        row["state"] = json!(format!(
            "judged under {}",
            if mode == "memory" { "memory" } else { "disk" }
        ));
        let out = clasify(&runtime, vec![row]).await;
        assert_eq!(
            out["queries"][0].get("replayed").cloned(),
            replayed.map(Value::from),
            "{mode}"
        );
        runtime.close().await;
    }
}

#[tokio::test]
async fn clasify_pages_complete_answers_once_and_replays_without_inference() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let labels: serde_json::Map<_, _> = (0..64)
        .map(|index| (format!("label{index:02}"), json!("criterion")))
        .collect();
    let questions: serde_json::Map<_, _> = (0..12)
        .map(|index| {
            (
                format!("question{index:02}"),
                json!({"type":"choice","criteria":labels,"instructions":"Choose one label."}),
            )
        })
        .collect();
    let rows: Vec<_> = (0..3)
        .map(|index| {
            json!({"id":format!("row{index}"),
        "state":{"text":format!("Evidence {index}")},"questions":questions})
        })
        .collect();
    let first = runtime
        .run(
            "clasify-pages".into(),
            "clasify".into(),
            json!({"queries":rows,"responseLength":1800}),
        )
        .await
        .expect("first page");
    let mut page = first.structured_content;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        assert!(pages < 60, "page chain must progress");
        for row in page["queries"].as_array().expect("query rows") {
            for (question, answer) in row["answers"].as_object().expect("answers") {
                assert_eq!(answer["probabilities"].as_object().unwrap().len(), 64);
                assert!(
                    seen.insert((row["id"].as_str().unwrap().to_owned(), question.clone())),
                    "duplicate answer"
                );
            }
        }
        pages += 1;
        let pagination = &page["responsePagination"];
        if pagination["hasMore"] != true {
            break;
        }
        let next = pagination["next"]["query"].clone();
        support::assert_cursor_only(&next);
        let next_page = runtime
            .run(format!("page{pages}"), "clasify".into(), next.clone())
            .await
            .expect("page");
        let repeated = runtime
            .run(format!("repeat{pages}"), "clasify".into(), next)
            .await
            .expect("repeat");
        assert_eq!(next_page.structured_content, repeated.structured_content);
        assert_eq!(
            next_page.structured_content["responsePagination"]["replayed"],
            true
        );
        page = next_page.structured_content;
    }
    assert_eq!(seen.len(), 36);
    assert!(pages > 1);
    assert_eq!(sent(&server).await.len(), 3);
    runtime.close().await;
}

/// A context selection that outlives its deadline (the constant
/// `RuntimeLimits::selection.timeout`, here 100 ms through the test seam)
/// discloses the gap instead of returning raw evidence: the receipt is
/// partial or unavailable, the unjudged evidence stays unknown with no body,
/// and `next.unfiltered` recovers it without the provider. Replaces the
/// read-tool-selection harness case that set OCTOCODE_CLASSIFICATION_TIMEOUT_MS.
#[tokio::test]
async fn a_selection_past_its_deadline_withholds_bodies_and_recovers() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            // A decisive answer: every question at its top level.
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .iter()
                .map(|(id, question)| {
                    let levels = question["criteria"].as_array().expect("levels");
                    let top = levels.len() - 1;
                    let legend = levels
                        .iter()
                        .enumerate()
                        .map(|(index, level)| (index.to_string(), level.clone()))
                        .collect::<serde_json::Map<_, _>>();
                    let probabilities = (0..levels.len())
                        .map(|index| (index.to_string(), json!(if index == top { 1.0 } else { 0.0 })))
                        .collect::<serde_json::Map<_, _>>();
                    (id.clone(), json!({"type":"score","score":top as f64,"confidence":1.0,
                        "legend":legend,"probabilities":probabilities}))
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200)
                .set_body_json(json!({"model":"m","answers":answers,"usage":{"input_tokens":1,"output_tokens":1}}))
                .set_delay(std::time::Duration::from_millis(400))
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "src/rule.txt",
        "UNKNOWN_SELECTION_SENTINEL cancellation removes a queued request.\n",
    );
    let read = |timeout: std::time::Duration| {
        let limits = octocode_native::execution::RuntimeLimits {
            selection: octocode_native::execution::SelectionLimits {
                timeout,
                ..crate::support::PROVIDER_LIMITS.selection
            },
            ..crate::support::PROVIDER_LIMITS
        };
        let runtime = workspace.runtime_with(&provider_env(&server, &[]), limits);
        let query = json!({"path": file, "clasifyContext":{"classifiers":["Does this state the cancellation rule?"]}});
        async move {
            let row = crate::support::run(&runtime, "localFetch", query, &[]).await;
            runtime.close().await;
            row.data().clone()
        }
    };
    let late = read(std::time::Duration::from_millis(100)).await;
    let selection = &late["selection"];
    assert_eq!(selection["status"], "partial", "{late}");
    assert_eq!(selection["unknownItems"], 1, "{late}");
    assert_eq!(selection["selectedItems"], 0, "{late}");
    // No static reminder: the receipt and `next.unfiltered` are the
    // per-call facts; "not proof of absence" is said once, in the
    // instructions.
    assert!(late.get("warnings").is_none(), "{late}");
    assert!(
        !late.to_string().contains("UNKNOWN_SELECTION_SENTINEL"),
        "{late}"
    );
    let unfiltered = late["next"]["unfiltered"].clone();
    assert_eq!(unfiltered["tool"], "localFetch", "{late}");
    let recovery = workspace.runtime(&[]);
    let recovered = recovery
        .run(
            "unfiltered".into(),
            "localFetch".into(),
            unfiltered["query"].clone(),
        )
        .await
        .expect("unfiltered recovery");
    assert!(
        recovered
            .structured_content
            .to_string()
            .contains("UNKNOWN_SELECTION_SENTINEL"),
        "{}",
        recovered.structured_content
    );
    recovery.close().await;
    // The same read under the constant deadline is judged and selected: the
    // deadline, not the evidence, left it unknown.
    let judged = read(
        octocode_native::execution::RuntimeLimits::DEFAULT
            .selection
            .timeout,
    )
    .await;
    assert_eq!(judged["selection"]["status"], "complete", "{judged}");
    assert_eq!(judged["selection"]["selectedItems"], 1, "{judged}");
    assert!(
        judged["content"]
            .as_str()
            .unwrap()
            .contains("UNKNOWN_SELECTION_SENTINEL"),
        "{judged}"
    );
}
