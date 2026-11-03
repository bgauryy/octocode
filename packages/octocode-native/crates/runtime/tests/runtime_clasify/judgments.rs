//! State-row judgments: the provider exchange, models, and fresh evaluation on every request in every storage mode.

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
async fn an_identical_evaluation_reaches_the_provider_every_time() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let first = clasify(&runtime, vec![content_row()]).await;
    let again = clasify(&runtime, vec![content_row()]).await;
    for out in [&first, &again] {
        assert!(out["results"][0]["data"].get("replayed").is_none(), "{out}");
    }
    assert_eq!(
        again["results"][0]["data"]["answers"],
        first["results"][0]["data"]["answers"]
    );
    assert_eq!(
        again["results"][0]["data"]["usage"],
        json!({"input_tokens":40,"output_tokens":7}),
        "a repeat is new spend"
    );
    // A different question is a different evaluation.
    let mut other = content_row();
    other["questions"] = json!({"restart":noul("Does `document.text` forbid a restart?")});
    let changed = clasify(&runtime, vec![other]).await;
    assert!(changed["results"][0]["data"].get("replayed").is_none());
    assert_eq!(sent(&server).await.len(), 3);
    runtime.close().await;
}

#[tokio::test]
async fn identical_rows_in_one_call_share_one_provider_request() {
    let server = provider(3).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let mut other_model = content_row();
    other_model["model"] = json!("jev-1.14.0");
    let mut other_context = content_row();
    other_context["context"] = json!("Historical behavior");
    let rows = vec![
        content_row(),
        other_model,
        content_row(),
        other_context,
        content_row(),
    ];
    let out = clasify(&runtime, rows).await;
    let results = out["results"].as_array().unwrap();
    assert_eq!(results.len(), 5, "{out}");
    for twin in [2, 4] {
        assert_eq!(results[0]["data"], results[twin]["data"], "{out}");
        assert!(results[twin].get("status").is_none(), "{out}");
    }
    assert!(
        results
            .iter()
            .all(|row| row["data"].get("replayed").is_none()),
        "{out}"
    );
    // Rows differing in model or context are different evaluations; the
    // three identical rows share one request.
    assert_eq!(sent(&server).await.len(), 3, "{out}");
    runtime.close().await;
}

#[tokio::test]
async fn judgments_are_saved_in_no_storage_mode_so_every_runtime_asks_again() {
    let server = provider(4).await;
    let workspace = Workspace::new();
    let env = |mode: &str| [("OCTOCODE_STORAGE_MODE", mode.to_owned())];
    for mode in ["persistent", "persistent", "memory", "memory"] {
        let runtime = clasify_runtime(&workspace, &server, &env(mode));
        let out = clasify(&runtime, vec![content_row()]).await;
        assert!(
            out["results"][0]["data"].get("replayed").is_none(),
            "{mode}: {out}"
        );
        runtime.close().await;
    }
    assert_eq!(sent(&server).await.len(), 4);
}
