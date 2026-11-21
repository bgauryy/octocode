use crate::support::{self, Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

fn search(root: &str, extra: Value) -> Value {
    let mut query = json!({"path":root,"matchString":["needle"]});
    query
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    query
}

#[tokio::test]
async fn invalid_rows_are_isolated_without_guessing_a_repair() {
    let workspace = Workspace::new();
    workspace.write("src/a.txt", "needle\n");
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[search("src",json!({})),search("src",json_fixture(r#"{"serchText": "typo"}"#)),search("src",json_fixture(r#"{"view": "files"}"#))]});
    let outcome = runtime
        .run("isolate".into(), "localSearch".into(), input)
        .await
        .expect("partly valid batch");
    let rows = outcome.structured_content["results"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["index"], index);
    }
    assert_ne!(rows[0]["status"], "error");
    assert_ne!(rows[2]["status"], "error");
    assert_eq!(rows[1]["status"], "error");
    assert_eq!(rows[1]["data"]["errorCode"], "invalidInput");
    let error = rows[1]["data"]["error"].as_str().unwrap();
    assert!(
        error.contains("queries[1]") && error.contains("serchText"),
        "{error}"
    );
    assert!(
        support::checks::leads(&rows[1]).is_empty(),
        "invalid input must not become a guessed call: {}",
        rows[1]
    );
    assert!(!outcome.all_failed);
    support::checks::assert_output_contract("localSearch", &outcome.structured_content);
    runtime.close().await;
}

#[tokio::test]
async fn all_invalid_rows_and_invalid_envelopes_fail_before_execution() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for input in [
        json!({"queries":[search(".",json_fixture(r#"{"bogus": 1}"#)),search(".",json_fixture(r#"{"alsoBogus": 2}"#))]}),
        json!({"queries":(0..6).map(|_|search(".",json!({}))).collect::<Vec<_>>()}),
        json!({"queries":[search(".",json!({}))],"responseScope":"rows"}),
    ] {
        let error = runtime
            .run("invalid".into(), "localSearch".into(), input)
            .await
            .expect_err("strict request");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    runtime.close().await;
}

fn match_ids(output: &Value) -> Vec<String> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            row["data"]["files"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|file| {
                    file["matches"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|hit| format!("{}:{}", file["path"], hit["line"]))
                })
        })
        .collect()
}

#[tokio::test]
async fn automatic_pages_reach_all_matches_once_on_both_transports() {
    use octocode_native::runtime::Channel;
    for channel in [Channel::Cli, Channel::Mcp] {
        let workspace = Workspace::new();
        for index in 0..20 {
            workspace.write(
                &format!("src/f{index}.txt"),
                format!("needle {}\n", "wide evidence ".repeat(100)),
            );
        }
        let runtime = workspace.runtime_with(
            &[],
            octocode_runtime_base::execution::RuntimeLimits {
                auto_page_chars: 4000,
                ..support::FIXTURE_LIMITS
            },
        );
        let input = json!({"queries":[search("src",json!({}))]});
        let mut walker = support::checks::Walker::new("localSearch", input);
        while let Some(input) = walker.next_query() {
            let admission = runtime.admit("page".into()).expect("admission");
            let outcome = runtime
                .execute(admission, "localSearch".into(), input, channel)
                .await
                .expect("page");
            let output = outcome.structured_content.into_value();
            assert!(
                !output["results"]
                    .as_array()
                    .expect("indexed results")
                    .is_empty()
            );
            if let Some(pagination) = output.get("responsePagination") {
                assert!(
                    pagination.get("hasMore").is_none(),
                    "a response block states no hasMore: {pagination}"
                );
                if let Some(next) = pagination.get("next") {
                    support::assert_cursor_only(&next["input"]);
                }
            }
            walker.accept(support::checks::Page {
                rows: match_ids(&output),
                output,
            });
        }
        let walk = walker.finish();
        assert!(walk.queries.len() > 1);
        assert_eq!(walk.rows.len(), 20);
        runtime.close().await;
    }
}

#[tokio::test]
async fn small_results_need_no_response_page() {
    let workspace = Workspace::new();
    workspace.write("one.txt", "needle\n");
    let runtime = workspace.runtime(&[]);
    let output = support::run(&runtime, "localSearch", search("one.txt", json!({}))).await;
    assert!(
        output
            .structured_content
            .get("responsePagination")
            .is_none()
    );
    assert_eq!(match_ids(&output.structured_content).len(), 1);
    runtime.close().await;
}

#[tokio::test]
async fn a_paged_partial_batch_discloses_oversized_pages_and_returns_rejections_once() {
    let workspace = Workspace::new();
    workspace.write(
        "large.txt",
        (0..100)
            .map(|index| format!("needle evidence {index}\n"))
            .collect::<String>(),
    );
    let runtime = workspace.runtime(&[]);
    let mut next = Some(
        json!({"queries":[search("large.txt",json!({})),search("large.txt",json!({"unknown":"x".repeat(200)}))],"responseChars":800}),
    );
    let mut rejected = 0;
    let mut pages = 0;
    while let Some(input) = next.take() {
        let output = runtime
            .run("batch-page".into(), "localSearch".into(), input)
            .await
            .expect("isolated batch")
            .structured_content;
        support::checks::assert_output_contract("localSearch", &output);
        let pagination = &output;
        // The rejected row rides page 1 outside its plan and measure.
        let mut planned = output.clone().into_value();
        if let Some(rows) = planned["results"].as_array_mut() {
            rows.retain(|row| row["index"] != 1);
        }
        planned.as_object_mut().unwrap().remove("oversized");
        let chars = octocode_runtime_base::response::pager::budgeted_chars(&planned).expect("page");
        assert!(pagination.get("returnedChars").is_none());
        assert!(pagination.get("totalChars").is_none());
        assert!(pagination.get("oversized").is_none(), "{output}");
        assert_eq!(
            output.get("oversized") == Some(&json!(true)),
            octocode_runtime_base::response::pager::holds_value_over(
                planned.as_object().expect("envelope"),
                octocode_runtime_base::id::ToolId::LocalSearch,
                800
            ),
            "{chars}: {output}"
        );
        rejected += output["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["index"] == 1)
            .count();
        next = pagination.pointer("/next/input").cloned();
        pages += 1;
        assert!(pages < 200);
    }
    assert_eq!(rejected, 1);
    assert!(pages > 1);
    runtime.close().await;
}

/// What a response publishes beside a row's list page shares its window:
/// a fresh request's input-repair warning and a rejected row. A list page
/// cut to its window fits beside them, so the response is one page of whole
/// rows, never a part of a row that would have fit.
#[tokio::test]
async fn a_list_page_fits_beside_repair_notes_and_rejected_rows() {
    let workspace = Workspace::new();
    let source = (0..120)
        .map(|n| format!("export function f{n}() {{ return {n}; }}\n"))
        .collect::<String>();
    workspace.write("app/a.ts", source);
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.join("app");
    let root = root.to_str().expect("path");
    runtime
        .run(
            "build".into(),
            "graphBuild".into(),
            json!({"queries":[{"path":root}]}),
        )
        .await
        .expect("graphBuild");
    let symbols = |extra: Value| {
        let mut row = json!({"operation":"symbols","path":root});
        row.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        row
    };
    for (window, input) in [
        // A repaired `limit` is read as 7, and the read is a warning.
        (
            1_200,
            json!({"queries":[symbols(json_fixture(r#"{"limit": "7"}"#))],"responseChars":1_200}),
        ),
        // `offset` is private progress: the second row is rejected.
        (
            3_000,
            json!({"queries":[symbols(json!({})),symbols(json_fixture(r#"{"offset": 10}"#))],"responseChars":3_000}),
        ),
    ] {
        let outcome = runtime
            .run("beside".into(), "graphQuery".into(), input.clone())
            .await
            .expect("graphQuery");
        let output = &outcome.structured_content;
        let text = serde_json::to_string(output).expect("json");
        let first = &output["results"][0];
        // The list page fits whole: the pager does not split it. Its own
        // next page is the reply's `next`, served with no saved response.
        assert!(
            first.get("rowPart").is_none() && output.get("expiresAt").is_none(),
            "{input}: the row was cut again: {text}"
        );
        assert!(
            first["data"]["rows"]
                .as_array()
                .is_some_and(|rows| rows.len() > 1),
            "{text}"
        );
        assert!(output["next"].is_object(), "{text}");
        assert!(text.len() <= window, "{input}: {} chars", text.len());
        assert!(
            output.get("warnings").is_some() || output["results"][1]["status"] == "error",
            "{text}"
        );
    }
    runtime.close().await;
}
