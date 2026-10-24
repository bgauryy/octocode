//! Complete source evidence survives response paging and intentional selection.
use crate::support::{self, Run as _, Workspace};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const WINDOW: usize = 20_000;

fn numbered_file(tag: &str, lines: usize) -> String {
    (1..=lines)
        .map(|line| format!("{tag} line {line:04} {}\n", "x".repeat(40)))
        .collect()
}

fn records(output: &Value) -> Vec<(String, usize, String)> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            let file = std::path::Path::new(row["data"]["path"].as_str().expect("source path"))
                .file_name()
                .expect("file")
                .to_string_lossy()
                .into_owned();
            row["data"]["content"]
                .as_str()
                .into_iter()
                .flat_map(str::lines)
                .filter_map(move |line| {
                    if line == "--" {
                        return None;
                    }
                    let (number, text) = line
                        .split_once('\t')
                        .unwrap_or_else(|| panic!("unnumbered source: {line:?}"));
                    Some((
                        file.clone(),
                        number.parse().expect("line number"),
                        text.to_owned(),
                    ))
                })
        })
        .collect()
}

async fn read_all(
    runtime: &octocode_native::runtime::ToolRuntime,
    input: Value,
) -> BTreeMap<(String, usize), String> {
    let mut walker = support::checks::Walker::new("localFetch", input);
    let mut found = BTreeMap::new();
    while let Some(input) = walker.next_query() {
        let output = runtime
            .run("read-page".into(), "localFetch".into(), input)
            .await
            .expect("read")
            .structured_content
            .into_value();
        let mut ids = Vec::new();
        for (path, line, text) in records(&output) {
            ids.push(format!("{path}:{line}"));
            assert!(
                found.insert((path, line), text).is_none(),
                "source line duplicated"
            );
        }
        walker.accept(support::checks::Page { output, rows: ids });
    }
    found
}

#[tokio::test]
async fn batched_reads_deliver_every_source_line_once_across_budgets() {
    let workspace = Workspace::new();
    let mut queries = Vec::new();
    let mut expected = BTreeMap::new();
    for (name, count) in [
        ("small.txt", 2),
        ("a.txt", 400),
        ("b.txt", 400),
        ("c.txt", 400),
    ] {
        let source = numbered_file(name, count);
        for (line, text) in source.lines().enumerate() {
            expected.insert((name.to_owned(), line + 1), text.to_owned());
        }
        queries.push(json!({"path":workspace.write(name,source)}));
    }
    let runtime = workspace.runtime(&[]);
    for budget in [2500, 20000] {
        let actual = read_all(&runtime, json!({"queries":queries,"responseChars":budget})).await;
        assert_eq!(actual, expected, "budget {budget}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn range_pages_deliver_exactly_the_requested_source_lines() {
    let workspace = Workspace::new();
    let source = numbered_file("big", 2000);
    let path = workspace.write("big.txt", &source);
    let runtime = workspace.runtime(&[]);
    let expected: BTreeMap<_, _> = source
        .lines()
        .enumerate()
        .filter(|(i, _)| (10..=20).contains(&(i + 1)) || (100..=1900).contains(&(i + 1)))
        .map(|(i, text)| (("big.txt".to_owned(), i + 1), text.to_owned()))
        .collect();
    for budget in [2500, 20000] {
        let actual = read_all(&runtime,json!({"queries":[{"path":path,"ranges":[{"startLine":10,"endLine":20},{"startLine":100,"endLine":1900}]}],"responseChars":budget})).await;
        assert_eq!(actual, expected, "budget {budget}");
    }
    runtime.close().await;
}

/// Unknown file settings cannot change the automatic response window.
#[tokio::test]
async fn workspace_config_cannot_change_the_response_window() {
    let workspace = Workspace::new();
    let wide: String = (0..200)
        .map(|line| format!("{line:03} {}\n", "x".repeat(2_000)))
        .collect();
    let file = workspace.write("wide.txt", &wide);
    workspace.write(".octocode/.env", "UNBOUND_RESPONSE_CHARS=50000\n");
    workspace.write(
        ".octocode/.octocoderc",
        r#"{"output":{"responseChars":50000}}"#,
    );
    std::fs::write(
        workspace.home.join(".octocoderc"),
        r#"{"output":{"responseChars":4000}}"#,
    )
    .expect("home rc");
    let base = workspace.config(&[]);
    let input = octocode_native::config::acquire_config_input(
        base.env,
        base.cwd,
        base.os_home,
        base.runtime_surface,
    );
    let runtime = octocode_native::runtime::ToolRuntime::new(input).expect("runtime");
    let diagnostics = &runtime.config().diagnostics;
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == "unknown_or_future_config")
    );
    let mcp = runtime
        .run_mcp(
            "window".into(),
            "localFetch".into(),
            json!({"queries":[{"path": file}]}),
        )
        .await
        .expect("read");
    let envelope = &mcp["structuredContent"];
    let page = envelope.to_string().encode_utf16().count();
    assert!(page <= WINDOW + 1_000, "page {page} exceeds the constant");
    runtime.close().await;
}

const SOURCE: &str = "\
pub fn first(value: usize) -> usize {
    let mut total = 0;
    for step in 0..value {
        total += step;
    }
    total += 1;
    total += 2;
    total += 3;
    total += 4;
    total += 5;
    total += 6;
    let marker = \"needle here\";
    total += 7;
    total += 8;
    total += 9;
    total += 10;
    total += 11;
    total += 12;
    total += 13;
    total
}

pub fn second() -> usize {
    let short = \"other needle\";
    short.len()
}
";

#[tokio::test]
async fn match_context_is_explicit_and_block_selects_the_whole_declaration() {
    let workspace = Workspace::new();
    let path = workspace.write("src/lib.rs", SOURCE);
    let runtime = workspace.runtime(&[]);
    for (fields, expected) in [
        (json!({"matchString":["needle here"]}), vec![12]),
        (
            json!({"matchString":["needle here"],"contextLines":{"before":2,"after":1}}),
            (10..=13).collect(),
        ),
        (json!({"matchString":["other needle"]}), vec![24]),
        (
            json!({"matchString":["needle here"],"block":true}),
            (1..=21).collect(),
        ),
    ] {
        let mut query = json!({"path":path});
        query
            .as_object_mut()
            .expect("row")
            .extend(fields.as_object().expect("fields").clone());
        let actual = read_all(&runtime, json!({"queries":[query]})).await;
        assert_eq!(
            actual.keys().map(|(_, line)| *line).collect::<Vec<_>>(),
            expected,
            "{fields}"
        );
        for ((_, line), text) in actual {
            assert_eq!(Some(text.as_str()), SOURCE.lines().nth(line - 1));
        }
    }
    runtime.close().await;
}
