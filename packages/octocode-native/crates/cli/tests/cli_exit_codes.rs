//! One shell exit code per tool-call outcome: 0 success (a next page is still
//! success), 1 empty, 2 rejected input, 3 not found, 4
//! authentication/permission, 5 execution failure, 7 rate limited, 130
//! interrupted.

use crate::support;
use serde_json::{Value, json};
use std::process::{Command, Output};
use support::Workspace;
mod providers;

fn run(mut command: Command, tool: &str, input: &Value) -> Output {
    command.args([tool, &input.to_string()]).output().unwrap()
}

fn run_json(mut command: Command, tool: &str, input: &Value) -> Output {
    command
        .args([tool, "--json", &input.to_string()])
        .output()
        .unwrap()
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("JSON output: {error}; {output:?}"))
}

fn first_data(output: &Output) -> Value {
    json_output(output)["results"][0]["data"].clone()
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn search(path: &str, pattern: &str) -> Value {
    json!({"path":path,"matchString":[pattern]})
}

fn fetch(path: &str) -> Value {
    json!({"path":path})
}

#[test]
fn local_rows_exit_by_outcome() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let file = file.to_string_lossy().into_owned();
    let missing = workspace
        .workspace
        .join("src/missing.txt")
        .to_string_lossy()
        .into_owned();
    let lines: String = (1..40).map(|n| format!("line {n}\n")).collect();
    let long = workspace
        .write("long.txt", lines)
        .to_string_lossy()
        .into_owned();
    let page = json!({"path":long});
    let cases = [
        (
            "localSearch",
            json!({"queries":[search(&root, "needle")]}),
            0,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "zzqqxx")]}),
            1,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "zzqqxx"), search(&root, "qqzzxx")]}),
            1,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "needle"), search(&root, "zzqqxx")]}),
            0,
        ),
        // Paging is success: the page's `next.*` is the continuation.
        ("localFetch", json!({"queries":[page.clone()]}), 0),
        ("localFetch", json!({"queries":[fetch(&missing)]}), 3),
        (
            "localFetch",
            json!({"queries":[fetch(&missing), fetch(&missing)]}),
            3,
        ),
        // A failed row is never hidden by an answering one: the worst row
        // decides (error class over a continuation).
        (
            "localFetch",
            json!({"queries":[fetch(&missing), fetch(&file)]}),
            3,
        ),
        ("localFetch", json!({"queries":[fetch(&missing), page]}), 3),
        (
            "localSearch",
            json!({"queries":[search(&root, "needle"), {"path":root,"bogus":1}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "zzqqxx"), {"path":root,"bogus":1}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[{"path":root,"bogus":1}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[{"path":root,"bogus":1}, {"path":root,"bogus":2}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[{"path":root,"matchString": ["("],"regexEngine":"rust"}]}),
            2,
        ),
    ];
    for (tool, input, expected) in cases {
        let output = run(workspace.cli(), tool, &input);
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{tool} {input}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

/// The root help states the batch exit rule the exit classification applies,
/// and each of its clauses holds on a real batch: a rejected row decides (2),
/// then a failed row (its code), then empty only when every row is empty
/// (1); an answering row beside an empty one exits 0.
#[test]
fn root_help_states_the_batch_exit_rule_the_cli_applies() {
    let workspace = Workspace::new();
    let help = workspace.cli().arg("--help").output().expect("help");
    let help = String::from_utf8_lossy(&help.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        help.contains(
            "A batch exits 2 if any row rejected its input, else its failed rows' code (5 if they differ), else 1 only when every row is empty or out of range, else 0; stdout still carries every row's result."
        ),
        "{help}"
    );
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let missing = workspace
        .workspace
        .join("src/missing.txt")
        .to_string_lossy()
        .into_owned();
    let rejected = json!({"path":root,"bogus":1});
    for (tool, rows, expected) in [
        (
            "localSearch",
            json!([search(&root, "needle"), search(&root, "zzqqxx")]),
            0,
        ),
        (
            "localSearch",
            json!([search(&root, "zzqqxx"), search(&root, "qqzzxx")]),
            1,
        ),
        (
            "localFetch",
            json!([fetch(&missing), fetch(&file.to_string_lossy())]),
            3,
        ),
        ("localSearch", json!([search(&root, "zzqqxx"), rejected]), 2),
    ] {
        let input = json!({"queries": rows});
        let output = run(workspace.cli(), tool, &input);
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{tool} {input}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn invalid_provider_configuration_uses_the_one_config_code() {
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command
        .env("OCTOCODE_STORAGE_MODE", "persistent")
        .env("GITHUB_API_URL", "https://user:pass@example.com/api/v3");
    let output = run_json(
        command,
        "ghCloneRepo",
        &json!({"queries":[{"owner":"a","repo":"b"}]}),
    );
    assert_exit(&output, 5);
    assert_eq!(first_data(&output)["errorCode"], "config");
}

#[test]
fn source_size_gaps_share_the_file_too_large_reason() {
    let workspace = Workspace::new();
    let utf16 = workspace.workspace.join("large-utf16.txt");
    let mut head = vec![0xff, 0xfe];
    for unit in "a".repeat(8192).encode_utf16() {
        head.extend(unit.to_le_bytes());
    }
    std::fs::write(&utf16, head).expect("UTF-16 text sample");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&utf16)
        .expect("open UTF-16 source")
        .set_len(10 * 1024 * 1024 + 2)
        .expect("grow UTF-16 source past the whole-file decode guard");
    let fetched = run_json(
        workspace.cli(),
        "localFetch",
        &json!({"queries":[fetch(&utf16.to_string_lossy())]}),
    );
    assert_exit(&fetched, 5);
    let data = first_data(&fetched);
    assert_eq!(data["errorCode"], "fileTooLarge", "{data}");
    assert_eq!(data["partialReasons"], json!(["fileTooLarge"]), "{data}");

    let searched = workspace.workspace.join("oversize-search.txt");
    std::fs::File::create(&searched)
        .expect("create search source")
        .set_len(20 * 1024 * 1024 + 1)
        .expect("grow search source past the multiline ceiling");
    let output = run_json(
        workspace.cli(),
        "localSearch",
        &json!({"queries":[{"path":searched,"matchString":["needle"],"multiline":"on"}]}),
    );
    assert_exit(&output, 1);
    let data = first_data(&output);
    assert_eq!(data["partialReasons"], json!(["fileTooLarge"]), "{data}");
}

#[test]
fn cli_tool_executes_despite_mcp_local_and_tool_filters() {
    let workspace = Workspace::new();
    let source = workspace.write("source.txt", "verified needle\n");
    let mut command = workspace.cli();
    command
        .env("OCTOCODE_ENABLE_LOCAL", "false")
        .env("TOOLS_TO_RUN", "ghSearchCode")
        .env("DISABLE_TOOLS", "localSearch");
    let output = run(
        command,
        "localSearch",
        &json!({"queries":[search(source.to_str().expect("source path"), "needle")]}),
    );
    assert_exit(&output, 0);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("verified needle"),
        "{output:?}"
    );
}
