use crate::support::{self, Run as _, Workspace};
use octocode_contract_types::ErrorCode;
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
    let input = json!({"queries":[search("src",json!({})),search("src",json!({"serchText":"typo"})),search("src",json!({"view":"files"}))]});
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
        json!({"queries":[search(".",json!({"bogus":1})),search(".",json!({"alsoBogus":2}))]}),
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
            octocode_native::execution::RuntimeLimits {
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
                assert!(pagination["page"].as_u64().is_some());
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
async fn a_paged_partial_batch_counts_rejected_rows_in_its_rendered_size_once() {
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
        let pagination = &output["responsePagination"];
        let chars = output.to_string().encode_utf16().count();
        assert_eq!(pagination["returnedChars"], chars);
        if chars > 800 {
            assert_eq!(pagination["oversized"], true);
        }
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
