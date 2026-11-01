//! Clasify response pages and deadlines: complete answers once, replays
//! without inference, and bodies withheld past a deadline then recovered.

use super::{provider, sent};
use crate::support::{self, Run as _, Workspace, clasify_runtime, provider_env};
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

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
            json!({"operation":"evaluate",
        "content":{"text":format!("Evidence {index}")},"questions":questions})
        })
        .collect();
    let first = runtime
        .run(
            "clasify-pages".into(),
            "clasify".into(),
            json!({"queries":rows,"responseChars":1800}),
        )
        .await
        .expect("first page");
    let mut page = first.structured_content;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        assert!(pages < 60, "page chain must progress");
        for row in page["results"].as_array().expect("result rows") {
            for (question, answer) in row["data"]["answers"].as_object().expect("answers") {
                assert_eq!(answer["probabilities"].as_object().unwrap().len(), 64);
                assert!(
                    seen.insert((row["index"].as_u64().unwrap(), question.clone())),
                    "duplicate answer"
                );
            }
        }
        pages += 1;
        let pagination = &page["responsePagination"];
        if pagination["hasMore"] != true {
            break;
        }
        let next = pagination["next"]["input"].clone();
        support::assert_cursor_only(&next);
        let next_page = runtime
            .run(format!("page{pages}"), "clasify".into(), next.clone())
            .await
            .expect("page");
        let repeated = runtime
            .run(format!("repeat{pages}"), "clasify".into(), next)
            .await
            .expect("repeat");
        let evidence = |outcome: &octocode_native::response::outcome::ToolOutcome| {
            let mut value = outcome.structured_content.clone().into_value();
            if let Some(pagination) = value["responsePagination"].as_object_mut()
                && let Some(next) = pagination.remove("next")
            {
                support::assert_cursor_only(&next["input"]);
            }
            // Each read restarts the saved response's lifetime and states
            // the expiry it set.
            if let Some(pagination) = value["responsePagination"].as_object_mut() {
                pagination.remove("expiresAt");
            }
            value
        };
        assert_eq!(evidence(&next_page), evidence(&repeated));
        let expiry = |outcome: &octocode_native::response::outcome::ToolOutcome| {
            outcome.structured_content["responsePagination"]["expiresAt"]
                .as_str()
                .map(str::to_owned)
        };
        assert!(
            expiry(&repeated) >= expiry(&next_page),
            "a later read never shortens retention"
        );
        assert_eq!(
            next_page.structured_content["responsePagination"]["replayed"],
            true
        );
        page = if pages % 2 == 0 {
            repeated.structured_content
        } else {
            next_page.structured_content
        };
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
/// and `next.unfiltered` recovers it in the same runtime without the provider.
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
        let query = json!({"path": file, "classification":{"predicates":["Does this state the cancellation rule?"]}});
        async move {
            let row = crate::support::run(&runtime, "localFetch", query).await;
            (runtime, row.data().clone())
        }
    };
    let (recovery, late) = read(std::time::Duration::from_millis(100)).await;
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
    let inference_before = sent(&server).await.len();
    let recovered = recovery
        .run(
            "unfiltered".into(),
            "localFetch".into(),
            unfiltered["input"].clone(),
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
    assert_eq!(
        sent(&server).await.len(),
        inference_before,
        "unfiltered recovery performs no inference"
    );
    recovery.close().await;
    // The same read under the constant deadline is judged and selected: the
    // deadline, not the evidence, left it unknown.
    let (runtime, judged) = read(
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
    runtime.close().await;
}
