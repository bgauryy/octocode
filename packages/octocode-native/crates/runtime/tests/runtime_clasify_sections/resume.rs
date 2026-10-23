//! Section resume and replay: current query ids, sibling continuations, and one transaction across independent runtimes.

use super::{provider, provider_calls, sections};
use crate::support::{Run as _, Workspace, clasify_runtime};
use serde_json::json;

#[tokio::test]
async fn section_resume_and_replay_use_each_current_query_id() {
    let provider = provider().await;
    let workspace = Workspace::new();
    let source = (0..70)
        .map(|index| format!("pub fn section_{index}() {{ println!(\"restart {index}\"); }}\n"))
        .collect::<String>();
    workspace.write("sections.rs", &source);
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let first = sections(
        &runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"sections.rs","fullContent":true}}}),
    )
    .await;
    assert_eq!(first["id"], "s");
    let mut resume = first["next"]["nextSectionPage"]["query"]["queries"][0].clone();
    assert!(resume["assessment"].is_string(), "{first:#}");
    resume["id"] = json!("resume");
    let resumed = runtime
        .run(
            "resume-current-id".into(),
            "clasify".into(),
            json!({"queries":[resume.clone()]}),
        )
        .await
        .expect("resumed result")
        .structured_content;
    assert_eq!(resumed["queries"][0]["id"], "resume", "{resumed:#}");
    let calls = provider_calls(&provider).await;
    let replays = ["first-replay", "second-replay"].map(|id| {
        let mut query = resume.clone();
        query["id"] = json!(id);
        query
    });
    let replayed = runtime
        .run(
            "replay-current-ids".into(),
            "clasify".into(),
            json!({"queries":replays}),
        )
        .await
        .expect("replayed result")
        .structured_content;
    for (index, id) in ["first-replay", "second-replay"].iter().enumerate() {
        let row = &replayed["queries"][index];
        assert_eq!(row["id"], *id, "{replayed:#}");
        assert_eq!(row["replayed"], true, "{replayed:#}");
        assert_eq!(row["resources"], resumed["queries"][0]["resources"]);
    }
    assert_eq!(provider_calls(&provider).await, calls);
    runtime.close().await;
}

#[tokio::test]
async fn sibling_section_rows_keep_continuations_until_every_section_is_answered() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write(
        "many.rs",
        (0..70)
            .map(|i| format!("pub fn section_{i}() {{ println!(\"restart {i}\"); }}\n"))
            .collect::<String>(),
    );
    let runtime = clasify_runtime(
        &workspace,
        &provider,
        &[("OCTOCODE_STORAGE_MODE", "persistent".into())],
    );
    let rows = ["a", "b"].map(|id| json!({"id":id,"operation":"sections","resources":{"file":{"tool":"localFetch","query":{"path":"many.rs","fullContent":true}}},"questions":{"q":{"type":"noul","instructions":"Does this section say to restart?"}}}));
    let first = runtime
        .run(
            "shared-sections".into(),
            "clasify".into(),
            json!({"queries":rows}),
        )
        .await
        .expect("first batch")
        .structured_content;
    for initial in first["queries"].as_array().expect("rows") {
        let mut row = initial.clone();
        let mut seen = std::collections::HashSet::new();
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(pages <= 4, "continuation made no bounded progress: {row:#}");
            for section in row["resources"]["file"]["sections"]
                .as_array()
                .expect("sections")
            {
                assert_eq!(section["answers"]["q"]["status"], "answered", "{row:#}");
                assert!(
                    seen.insert(section["id"].as_str().expect("section id").to_owned()),
                    "section emitted twice"
                );
            }
            let next = &row["next"]["nextSectionPage"];
            if next.is_null() {
                break;
            }
            row = runtime
                .run(
                    format!("resume-{}-{pages}", initial["id"]),
                    "clasify".into(),
                    next["query"].clone(),
                )
                .await
                .expect("resume")
                .structured_content["queries"][0]
                .clone();
        }
        assert_eq!(seen.len(), 70, "{row:#}");
        assert_eq!(row["resources"]["file"]["coverage"]["unassessed"], 0);
    }
    runtime.close().await;
}

#[tokio::test]
async fn simultaneous_section_resume_in_independent_runtimes_replays_one_transaction() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write(
        "many.rs",
        (0..70)
            .map(|i| format!("pub fn section_{i}() {{ println!(\"restart {i}\"); }}\n"))
            .collect::<String>(),
    );
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let first_runtime = clasify_runtime(&workspace, &provider, &env);
    let second_runtime = clasify_runtime(&workspace, &provider, &env);
    let first = sections(
        &first_runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"many.rs","fullContent":true}}}),
    )
    .await;
    let next = first["next"]["nextSectionPage"]["query"].clone();
    assert!(next["queries"][0]["assessment"].is_string(), "{first:#}");
    let before = provider_calls(&provider).await;
    let (a, b) = tokio::join!(
        first_runtime.run("resume-a".into(), "clasify".into(), next.clone()),
        second_runtime.run("resume-b".into(), "clasify".into(), next)
    );
    let a = a.expect("first resume").structured_content;
    let b = b.expect("second resume").structured_content;
    assert_eq!(a["queries"][0]["resources"], b["queries"][0]["resources"]);
    assert_ne!(
        a["queries"][0]["replayed"] == true,
        b["queries"][0]["replayed"] == true,
        "{a:#} {b:#}"
    );
    assert_eq!(provider_calls(&provider).await - before, 6);
    first_runtime.close().await;
    second_runtime.close().await;
}
