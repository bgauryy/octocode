//! One response page carries the walks of the rows it split; a row shown
//! whole keeps its own `next.nextPage`.

use crate::support::{Run as _, Workspace};
use octocode_native::runtime::ToolRuntime;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

fn numbered(file: usize, lines: usize) -> String {
    (1..=lines)
        .map(|line| format!("needle file {file} line {line} {}\n", "padding ".repeat(4)))
        .collect()
}

async fn run(runtime: &ToolRuntime, input: Value) -> Value {
    let output = runtime
        .run("call".into(), "localSearch".into(), input)
        .await
        .expect("served")
        .structured_content
        .into_value();
    octocode_runtime_base::contracts::validate_output("localSearch", &output).expect("contract");
    output
}

fn row(output: &Value, index: u64) -> Option<&Value> {
    output["results"]
        .as_array()?
        .iter()
        .find(|row| row["index"] == index)
}

#[tokio::test]
async fn a_row_shown_whole_keeps_its_own_walk_beside_a_split_row() {
    let workspace = Workspace::new();
    for file in 0..6 {
        workspace.write(&format!("big/f{file}.txt"), numbered(file, 25));
    }
    for file in 0..8 {
        workspace.write(&format!("small/s{file}.txt"), "needle\n");
    }
    let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".to_owned())]);
    let mut page = run(
        &runtime,
        json_fixture(
            r#"{
                "queries": [
                    {
                        "path": "big",
                        "matchString": ["needle"],
                        "view": "content",
                        "limit": 40
                    },
                    {
                        "path": "small",
                        "matchString": ["needle"],
                        "view": "files",
                        "limit": 3
                    }
                ],
                "responseChars": 1500
            }"#,
        ),
    )
    .await;
    let small_walk = loop {
        if let Some(call) = row(&page, 1).and_then(|row| row.pointer("/data/next/nextPage")) {
            break call.clone();
        }
        let next = page
            .pointer("/responsePagination/next/input")
            .cloned()
            .unwrap_or_else(|| panic!("the small row keeps its walk: {page}"));
        page = run(&runtime, next).await;
    };
    // Finishing the response walk reads only the split row.
    let mut next = page.pointer("/responsePagination/next/input").cloned();
    while let Some(input) = next {
        let later = run(&runtime, input).await;
        assert!(row(&later, 1).is_none(), "{later}");
        next = later.pointer("/responsePagination/next/input").cloned();
    }
    let more = run(&runtime, small_walk["input"].clone()).await;
    assert_eq!(
        row(&more, 1).map(|row| row["data"]["pagination"]["offset"].clone()),
        Some(json!(3)),
        "the row's own walk serves its next page: {more}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn rows_whose_pages_fit_state_no_response_walk() {
    let workspace = Workspace::new();
    for file in 0..8 {
        workspace.write(&format!("a/a{file}.txt"), "needle\n");
        workspace.write(&format!("b/b{file}.txt"), "needle\n");
    }
    let runtime = workspace.runtime(&[]);
    let output = run(
        &runtime,
        json_fixture(
            r#"{
                "queries": [
                    {
                        "path": "a",
                        "matchString": ["needle"],
                        "view": "files",
                        "limit": 3
                    },
                    {
                        "path": "b",
                        "matchString": ["needle"],
                        "view": "files",
                        "limit": 3
                    }
                ]
            }"#,
        ),
    )
    .await;
    runtime.close().await;
    assert!(output.get("responsePagination").is_none(), "{output}");
    for index in [0, 1] {
        assert!(
            row(&output, index)
                .and_then(|row| row.pointer("/data/next/nextPage"))
                .is_some(),
            "row {index} keeps its walk: {output}"
        );
    }
}
