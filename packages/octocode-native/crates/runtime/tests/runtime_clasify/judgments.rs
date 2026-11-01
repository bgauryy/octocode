//! State-row judgments: the provider exchange, models, replay and single flight, and persistence.

use super::{clasify, content_row, noul, provider, sent};
use crate::support::{Workspace, clasify_runtime};
use serde_json::{Value, json};

#[tokio::test]
async fn a_content_row_sends_the_questions_unchanged_and_returns_provider_answers() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut row = content_row();
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
    assert_eq!(request["state"], row["content"]);
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

    let result = &out["results"][0]["data"];
    assert_eq!(out["results"][0]["index"], 0);
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
    let mut pinned = content_row();
    pinned["model"] = json!("jev-1.14.0");
    clasify(&runtime, vec![content_row(), pinned]).await;
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
async fn research_context_wraps_the_state_once_and_changes_the_cache_identity() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    for context in ["  Current production behavior\n", "Historical behavior"] {
        let mut row = content_row();
        row["context"] = json!(context);
        clasify(&runtime, vec![row.clone()]).await;
        let requests = sent(&server).await;
        let request = requests.last().unwrap();
        assert_eq!(
            request["state"],
            json!({"context":context,"content":row["content"]})
        );
        for question in request["questions"].as_object().unwrap().values() {
            assert_eq!(question, &row["questions"]["restart"]);
        }
    }
    assert_eq!(sent(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn an_identical_evaluation_replays_without_a_provider_request() {
    let server = provider(2).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let first = clasify(&runtime, vec![content_row()]).await;
    let again = clasify(&runtime, vec![content_row()]).await;
    assert!(first["results"][0]["data"].get("replayed").is_none());
    assert_eq!(again["results"][0]["data"]["replayed"], true);
    assert_eq!(
        again["results"][0]["data"]["answers"],
        first["results"][0]["data"]["answers"]
    );
    assert_eq!(
        again["results"][0]["data"]["usage"],
        first["results"][0]["data"]["usage"]
    );
    // A different question is a different evaluation.
    let mut other = content_row();
    other["questions"] = json!({"restart":noul("Does `document.text` forbid a restart?")});
    let changed = clasify(&runtime, vec![other]).await;
    assert!(changed["results"][0]["data"].get("replayed").is_none());
    assert_eq!(sent(&server).await.len(), 2);
    runtime.close().await;
}

#[tokio::test]
async fn identical_rows_in_one_call_share_one_provider_request() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let second = content_row();
    let out = clasify(&runtime, vec![content_row(), second]).await;
    let rows = out["results"].as_array().unwrap();
    assert_eq!(rows[0]["data"]["answers"], rows[1]["data"]["answers"]);
    assert_eq!(
        rows.iter()
            .filter(|row| row["data"].get("replayed").is_some())
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
        let mut row = content_row();
        row["content"] = json!(format!(
            "judged under {}",
            if mode == "memory" { "memory" } else { "disk" }
        ));
        let out = clasify(&runtime, vec![row]).await;
        assert_eq!(
            out["results"][0]["data"].get("replayed").cloned(),
            replayed.map(Value::from),
            "{mode}"
        );
        runtime.close().await;
    }
}
