//! Section resume and replay preserve row indexes, sibling work, and one transaction across independent runtimes.

use super::{provider, provider_calls, sections};
use crate::support::{Run as _, Workspace, clasify_runtime};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};

async fn assert_same_resources(
    first_runtime: &ToolRuntime,
    first: &Value,
    replay_runtime: &ToolRuntime,
    replay: &Value,
) {
    let resources = |value: &Value| {
        let mut facts = value.as_object().expect("resources").clone();
        for resource in facts.values_mut() {
            resource.as_object_mut().expect("resource").remove("read");
        }
        facts
    };
    assert_eq!(resources(first), resources(replay));
    for (name, original) in first.as_object().expect("resources") {
        let mut captured = None;
        for (runtime, resource) in [(first_runtime, original), (replay_runtime, &replay[name])] {
            let read = &resource["read"];
            assert_eq!(read["kind"], "continue");
            crate::support::assert_cursor_only(&read["input"]);
            let result = runtime
                .run(
                    format!("read-{name}"),
                    read["tool"].as_str().expect("reader").into(),
                    read["input"].clone(),
                )
                .await
                .expect("captured evidence remains readable")
                .structured_content;
            let text = result["results"][0]["data"]["content"]
                .as_str()
                .expect("captured text");
            if let Some(expected) = &captured {
                assert_eq!(text, expected);
            } else {
                captured = Some(text.to_owned());
            }
        }
    }
}

#[tokio::test]
async fn section_cursor_replays_frozen_work_without_repeating_inference() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write("sections.rs", super::separate_functions(70));
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let first = sections(
        &runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"sections.rs"}}}),
    )
    .await;
    let resume = first["next"]["nextSectionPage"]["input"].clone();
    crate::support::assert_cursor_only(&resume);
    let resumed = runtime
        .run("resume".into(), "clasify".into(), resume.clone())
        .await
        .expect("resume")
        .structured_content;
    assert_eq!(resumed["results"][0]["index"], 0);
    let calls = provider_calls(&provider).await;
    for label in ["first replay", "second replay"] {
        let replay = runtime
            .run(label.into(), "clasify".into(), resume.clone())
            .await
            .expect("replay")
            .structured_content;
        assert_eq!(replay["results"][0]["index"], 0);
        assert_same_resources(
            &runtime,
            &resumed["results"][0]["data"]["resources"],
            &runtime,
            &replay["results"][0]["data"]["resources"],
        )
        .await;
    }
    assert_eq!(provider_calls(&provider).await, calls);
    runtime.close().await;
}

#[tokio::test]
async fn sibling_section_rows_keep_continuations_until_every_section_is_answered() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write("many.rs", super::separate_functions(70));
    let runtime = clasify_runtime(
        &workspace,
        &provider,
        &[("OCTOCODE_STORAGE_MODE", "persistent".into())],
    );
    let rows = [0, 1].map(|_| json!({"operation":"sections","resources":{"file":{"tool":"localFetch","query":{"path":"many.rs"}}},"questions":{"q":{"type":"noul","instructions":"Does this section say to restart?"}}}));
    let first = runtime
        .run(
            "shared-sections".into(),
            "clasify".into(),
            json!({"queries":rows}),
        )
        .await
        .expect("first batch")
        .structured_content;
    for initial in first["results"].as_array().expect("rows") {
        let mut row = initial["data"].clone();
        let mut seen = std::collections::HashSet::new();
        let mut pages = 0;
        loop {
            pages += 1;
            // Two rows share each call's admission budget: progress, never a stall.
            assert!(
                pages <= 70,
                "continuation made no bounded progress: {row:#}"
            );
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
                    format!("resume-{}-{pages}", initial["index"]),
                    "clasify".into(),
                    next["input"].clone(),
                )
                .await
                .expect("resume")
                .structured_content["results"][0]["data"]
                .clone();
        }
        assert_eq!(seen.len(), 70, "{row:#}");
        assert!(pages > 1, "70 page-sized sections need a continuation");
        assert_eq!(row["resources"]["file"]["coverage"]["unassessed"], 0);
    }
    runtime.close().await;
}

#[tokio::test]
async fn simultaneous_section_resume_in_independent_runtimes_replays_one_transaction() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write("many.rs", super::separate_functions(70));
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let first_runtime = clasify_runtime(&workspace, &provider, &env);
    let second_runtime = clasify_runtime(&workspace, &provider, &env);
    let first = sections(
        &first_runtime,
        json!({"file":{"tool":"localFetch","query":{"path":"many.rs"}}}),
    )
    .await;
    let next = first["next"]["nextSectionPage"]["input"].clone();
    crate::support::assert_cursor_only(&next);
    let before = provider_calls(&provider).await;
    let (a, b) = tokio::join!(
        first_runtime.run("resume-a".into(), "clasify".into(), next.clone()),
        second_runtime.run("resume-b".into(), "clasify".into(), next)
    );
    let a = a.expect("first resume").structured_content;
    let b = b.expect("second resume").structured_content;
    assert_same_resources(
        &first_runtime,
        &a["results"][0]["data"]["resources"],
        &second_runtime,
        &b["results"][0]["data"]["resources"],
    )
    .await;
    assert_ne!(
        a["results"][0]["data"]["replayed"] == true,
        b["results"][0]["data"]["replayed"] == true,
        "{a:#} {b:#}"
    );
    // One transaction: the page the two resumes share was judged once.
    let judged = a["results"][0]["data"]["resources"]["file"]["sections"]
        .as_array()
        .expect("sections")
        .len();
    assert!(judged > 0);
    assert_eq!(provider_calls(&provider).await - before, judged);
    first_runtime.close().await;
    second_runtime.close().await;
}
