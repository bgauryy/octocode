//! Section resume and replay preserve row indexes, sibling work, and one transaction across independent runtimes.

use super::{provider, provider_calls, sections};
use crate::support::{Run as _, Workspace, clasify_runtime};
use octocode_native::runtime::ToolRuntime;
use octocode_test_support::json_fixture;
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
            assert_eq!(read.get("kind"), None);
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
        json_fixture(
            r#"{
                "file": {"tool": "localFetch", "query": {"path": "sections.rs"}}
            }"#,
        ),
    )
    .await;
    let resume = first["next"]["nextPage"]["input"].clone();
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
    let rows = [0, 1].map(|_| {
        json_fixture(
            r#"{
                "operation": "sections", "maxRequests": 1000,
                "resources": {
                    "file": {"tool": "localFetch", "query": {"path": "many.rs"}}
                },
                "questions": {
                    "q": {
                        "type": "noul",
                        "instructions": "Does this section say to restart?"
                    }
                }
            }"#,
        )
    });
    let first = runtime
        .run(
            "shared-sections".into(),
            "clasify".into(),
            json!({"queries":rows}),
        )
        .await
        .expect("first batch")
        .structured_content;
    // Each row's sections, in the rows' one walk: progress for both, never a stall.
    let mut seen = [
        std::collections::HashSet::new(),
        std::collections::HashSet::new(),
    ];
    let (mut page, mut pages) = (first, 0);
    let mut queue = std::collections::VecDeque::new();
    loop {
        pages += 1;
        assert!(
            pages <= 140,
            "continuation made no bounded progress: {page}"
        );
        for result in page["results"].as_array().expect("rows") {
            let index = result["index"].as_u64().expect("index") as usize;
            for section in result["data"]["resources"]["file"]["sections"]
                .as_array()
                .into_iter()
                .flatten()
            {
                assert_eq!(section["answers"]["q"]["status"], "answered", "{result:#}");
                assert!(
                    seen[index].insert(section["id"].as_str().expect("section id").to_owned()),
                    "section emitted twice"
                );
            }
        }
        for result in page["results"].as_array().expect("rows") {
            if crate::support::next_inputs(&json!({"results":[result]})).is_empty() {
                assert!(
                    result["data"]["resources"]["file"]["coverage"]
                        .get("unassessed")
                        .is_none()
                );
            }
        }
        queue.extend(crate::support::next_inputs(&page));
        let Some(next) = queue.pop_front() else {
            break;
        };
        page = runtime
            .run(format!("resume-{pages}"), "clasify".into(), next)
            .await
            .expect("resume")
            .structured_content;
    }
    assert!(pages > 1, "70 page-sized sections need a continuation");
    assert_eq!((seen[0].len(), seen[1].len()), (70, 70));
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
        json_fixture(
            r#"{
                "file": {"tool": "localFetch", "query": {"path": "many.rs"}}
            }"#,
        ),
    )
    .await;
    let next = first["next"]["nextPage"]["input"].clone();
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

/// Every page of a first walk is that page's first delivery, however the
/// response pager splits a batch: no row of it says `replayed`, and a page
/// says it was read from the saved response exactly when a cursor opened it.
/// Sending a page's cursor again is a replay, and says so.
#[tokio::test]
async fn a_split_batch_is_first_delivered_unreplayed_and_a_repeat_is_replayed() {
    let provider = provider().await;
    let workspace = Workspace::new();
    workspace.write("many.rs", super::separate_functions(70));
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    let row = |path: &str| json!({"operation":"sections","maxRequests":crate::runtime_clasify::budget::EVERY_PAGE,"resources":{"file":{"tool":"localFetch","query":{"path":path}}},"questions":{"q":{"type":"noul","instructions":"Does this section say to restart?"}}});
    let mut page = runtime
        .run(
            "split-first".into(),
            "clasify".into(),
            json!({"queries":[row("many.rs"), row("many.rs")],"responseChars":2_500}),
        )
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let mut cursors: Vec<(Value, bool)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut billed = true;
    let mut queue = std::collections::VecDeque::new();
    loop {
        assert!(cursors.len() <= 400, "the walk makes bounded progress");
        // `responsePagination.replayed` states that the page was read from
        // the saved response, without acquisition or inference: every saved
        // page, its first delivery included; a page that judged a batch is
        // new work (CLS-C26, closed by decision).
        assert_eq!(
            page["responsePagination"]["replayed"] == true,
            !billed,
            "page {}: {page:#}",
            cursors.len()
        );
        if let Some((_, saved)) = cursors.last_mut() {
            *saved = !billed;
        }
        for result in page["results"].as_array().expect("rows") {
            assert!(
                result["data"].get("replayed").is_none(),
                "a first delivery is no replay (page {}): {result:#}",
                cursors.len()
            );
            let index = result["index"].as_u64().expect("index");
            for section in result["data"]["resources"]["file"]["sections"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let id = section["id"].as_str().expect("section id").to_owned();
                assert!(seen.insert((index, id)), "a section is delivered once");
            }
        }
        queue.extend(crate::support::next_inputs(&page));
        let Some(next) = queue.pop_front() else {
            break;
        };
        cursors.push((next.clone(), false));
        let calls = provider_calls(&provider).await;
        page = runtime
            .run(format!("split-{}", cursors.len()), "clasify".into(), next)
            .await
            .expect("next page")
            .structured_content
            .into_value();
        billed = provider_calls(&provider).await > calls;
    }
    assert!(
        cursors.iter().any(|(_, saved)| *saved),
        "a batch pages over saved response pages"
    );
    assert!(cursors.len() > 2, "the batches split over response pages");
    assert_eq!(seen.len(), 140, "every section of both rows is reached");
    let calls = provider_calls(&provider).await;
    for (cursor, saved_page) in cursors {
        let repeat = runtime
            .run("split-repeat".into(), "clasify".into(), cursor)
            .await
            .expect("repeat")
            .structured_content
            .into_value();
        // A saved page read again says so on the page; a batch sent again
        // says so on its rows.
        let replayed = if saved_page {
            repeat["responsePagination"]["replayed"] == true
        } else {
            repeat["results"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|row| row["data"]["replayed"] == true)
        };
        assert!(replayed, "a repeat says it is a replay: {repeat:#}");
    }
    assert_eq!(
        provider_calls(&provider).await,
        calls,
        "a repeat bills nothing"
    );
    runtime.close().await;
}

/// Answers Noul 0.9 for the section that holds `restart 0`, else 0.1: the
/// others are quiet.
async fn quiet_provider() -> wiremock::MockServer {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let hit = body["state"]["target"]["text"]
                .as_str()
                .is_some_and(|text| text.contains("restart 0"));
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        json!({"type":"noul","noul":if hit { 0.9 } else { 0.1 }}),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-1.13.0","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}
            }))
        })
        .mount(&server)
        .await;
    server
}

/// A replay never answers from text that changed: once the source is
/// edited, `next.details` and `next.quiet` each return `staleSnapshot` with
/// the plan's own request as `next.restart`, bill nothing, and serve no
/// saved answer; the restart judges the new text.
#[tokio::test]
async fn a_replay_over_a_changed_source_is_stale_and_restarts_the_plan() {
    let provider = quiet_provider().await;
    let workspace = Workspace::new();
    workspace.write("small.rs", super::separate_functions(2));
    let runtime = clasify_runtime(&workspace, &provider, &[]);
    // The summary view, which offers `next.details`.
    let row = json_fixture(
        r#"{
            "operation": "sections", "maxRequests": 1000,
            "view": "summary",
            "resources": {
                "file": {"tool": "localFetch", "query": {"path": "small.rs"}}
            },
            "questions": {
                "q": {
                    "type": "noul",
                    "instructions": "Does this section say to restart?"
                }
            }
        }"#,
    );
    let first = runtime
        .run("first".into(), "clasify".into(), json!({"queries":[row]}))
        .await
        .expect("first batch")
        .structured_content
        .into_value();
    let first = first["results"][0]["data"].clone();
    let replays = ["details", "quiet"].map(|channel| {
        let call = &first["next"][channel];
        assert_eq!(call["tool"], "clasify", "next.{channel}: {first:#}");
        (channel, call["input"].clone())
    });
    // Unchanged: each replays its saved answers.
    for (channel, input) in &replays {
        let replay = runtime
            .run(format!("{channel}-same"), "clasify".into(), input.clone())
            .await
            .expect("replay")
            .structured_content
            .into_value();
        assert_eq!(replay["results"][0]["data"]["replayed"], true, "{replay:#}");
    }
    workspace.write("small.rs", super::separate_functions(1));
    let calls = provider_calls(&provider).await;
    let mut restart = None;
    for (channel, input) in replays {
        let stale = runtime
            .run(format!("{channel}-changed"), "clasify".into(), input)
            .await
            .expect("typed row")
            .structured_content
            .into_value();
        let data = &stale["results"][0]["data"];
        assert_eq!(
            data["errorCode"], "staleSnapshot",
            "next.{channel}: {stale:#}"
        );
        assert!(
            data.get("resources").is_none(),
            "no saved answer: {stale:#}"
        );
        assert!(data.get("replayed").is_none(), "{stale:#}");
        assert!(
            data.get("hints").is_none(),
            "the restart is the recovery: {stale:#}"
        );
        let call = &data["next"]["restart"];
        assert_eq!(call["tool"], "clasify", "{stale:#}");
        assert_eq!(call.get("kind"), None, "{stale:#}");
        restart = Some(call["input"].clone());
    }
    assert_eq!(
        provider_calls(&provider).await,
        calls,
        "a stale replay bills nothing"
    );
    let restarted = runtime
        .run(
            "restart".into(),
            "clasify".into(),
            restart.expect("restart"),
        )
        .await
        .expect("restart")
        .structured_content
        .into_value();
    let file = &restarted["results"][0]["data"]["resources"]["file"];
    assert_eq!(file["status"], "ready", "{restarted:#}");
    assert_eq!(
        file["coverage"]["totalSections"], 1,
        "the new text: {restarted:#}"
    );
    assert!(
        provider_calls(&provider).await > calls,
        "a restart judges again"
    );
    runtime.close().await;
}
