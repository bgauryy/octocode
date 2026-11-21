//! One response page carries the walks of the rows it split; a row shown
//! whole keeps its own `next.nextPage`.

use crate::support::{Run as _, Workspace};
use octocode_native::runtime::ToolRuntime;
use octocode_test_support::json_fixture;
use serde_json::Value;

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
async fn one_loop_through_next_reads_a_split_row_and_a_whole_rows_later_pages() {
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
    // One loop through the reply's `next` reads everything: the split row's
    // parts, then the later pages of both rows. No row carries a walk of
    // its own.
    let mut small = Vec::new();
    let mut big = 0;
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls < 200, "the walk ends");
        for index in [0, 1] {
            assert!(
                row(&page, index)
                    .and_then(|row| row.pointer("/data/next/nextPage"))
                    .is_none(),
                "{page}"
            );
        }
        if let Some(row) = row(&page, 1) {
            small.extend(
                row["data"]["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|file| file["path"].as_str().map(str::to_owned)),
            );
        }
        if let Some(row) = row(&page, 0) {
            big += row["data"]["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|file| file["matches"].as_array().map_or(0, Vec::len))
                .sum::<usize>();
        }
        let Some(next) = page.pointer("/next/input").cloned() else {
            break;
        };
        page = run(&runtime, next).await;
    }
    small.sort();
    assert_eq!(
        small,
        (0..8)
            .map(|file| format!("small/s{file}.txt"))
            .collect::<Vec<_>>(),
        "every file of the whole row once"
    );
    assert_eq!(big, 6 * 25, "every hit of the split row once");
    runtime.close().await;
}

#[tokio::test]
async fn rows_whose_pages_fit_continue_together_on_the_replys_next() {
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
    assert!(output["next"]["input"]["cursor"].is_string(), "{output}");
    assert!(
        output.get("expiresAt").is_none(),
        "no saved response: {output}"
    );
    for index in [0, 1] {
        assert!(
            row(&output, index)
                .and_then(|row| row.pointer("/data/next/nextPage"))
                .is_none(),
            "row {index} carries no walk of its own: {output}"
        );
    }
}
