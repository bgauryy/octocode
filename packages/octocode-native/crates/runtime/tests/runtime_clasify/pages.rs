//! Clasify response pages and deadlines: complete answers once, replays
//! without inference, and bodies withheld past a deadline then recovered.

use super::{provider, sent};
use crate::support::{self, Run as _, Workspace, clasify_runtime};
use serde_json::json;

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
        let pagination = &page;
        if pagination["next"].is_null() {
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
        let evidence = |outcome: &octocode_runtime_base::response::outcome::ToolOutcome| {
            let mut value = outcome.structured_content.clone().into_value();
            if let Some(envelope) = value.as_object_mut() {
                if let Some(next) = envelope.remove("next") {
                    support::assert_cursor_only(&next["input"]);
                }
                // Each read restarts the saved response's lifetime and
                // states the expiry it set.
                envelope.remove("expiresAt");
            }
            value
        };
        assert_eq!(evidence(&next_page), evidence(&repeated));
        let expiry = |outcome: &octocode_runtime_base::response::outcome::ToolOutcome| {
            outcome.structured_content["expiresAt"]
                .as_str()
                .map(str::to_owned)
        };
        assert!(
            expiry(&repeated) >= expiry(&next_page),
            "a later read never shortens retention"
        );
        assert_eq!(next_page.structured_content["replayed"], true);
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
