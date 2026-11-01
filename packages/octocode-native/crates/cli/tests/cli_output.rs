//! What the CLI prints, by stream: JSON is indented for a terminal and one
//! compact line on a pipe; errors keep their whole message and one envelope;
//! a pipe-implied JSON error is also told to a person watching stderr.
//!
//! A terminal is a pseudo-terminal from `openpty`, given to the child as its
//! stdout or stderr. The terminal turns `\n` into `\r\n`; [`tty`] undoes it.

use crate::support::Workspace;
use serde_json::Value;
use std::process::Output;

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("UTF-8 stdout")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).expect("UTF-8 stderr")
}

/// The one compact JSON line a pipe gets, parsed back.
fn compact_line(text: &str) -> Value {
    let line = text.strip_suffix('\n').expect("newline-terminated");
    assert!(!line.contains('\n'), "one line: {text}");
    let value: Value = serde_json::from_str(line).expect("JSON line");
    assert_eq!(serde_json::to_string(&value).expect("compact"), line);
    value
}

/// A search hit in a fresh workspace: the query and its workspace.
fn search() -> (Workspace, &'static str) {
    let workspace = Workspace::new();
    workspace.write("notes.txt", "alpha needle omega\n");
    (
        workspace,
        r#"{"queries":[{"path":"notes.txt","matchString": ["needle"]}]}"#,
    )
}

/// Every JSON command answers a pipe with one compact line, as before.
#[test]
fn piped_json_is_one_compact_line() {
    let (workspace, query) = search();
    for args in [
        vec!["localSearch", query, "--json"],
        vec!["schema"],
        vec!["schema", "localSearch", "--view", "query", "--json"],
        vec!["config", "--json"],
        vec!["config", "home", "--json"],
        vec!["install", "--list", "--json"],
    ] {
        let output = workspace.cli().args(&args).output().expect("run");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
        compact_line(stdout(&output));
    }
}

#[test]
fn typed_context_controls_terminal_numbering_without_losing_source() {
    let workspace = Workspace::new();
    workspace.write("notes.txt", "alpha\nneedle here\nomega\n");
    let query = serde_json::json!({"queries":[{"path":".","matchString":["needle"],"contextLines":{"before":1,"after":1}}]});
    let output = workspace
        .cli()
        .args(["localSearch", &query.to_string()])
        .output()
        .expect("search");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "notes.txt\n1-alpha\n2:needle here\n3-omega\n"
    );
    let files =
        serde_json::json!({"queries":[{"path":".","matchString":["needle"],"view":"files"}]});
    let output = workspace
        .cli()
        .args(["localSearch", &files.to_string()])
        .output()
        .expect("files");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "notes.txt\n");
}

#[test]
fn terminal_cursor_commands_execute_in_a_later_process() {
    let workspace = Workspace::new();
    for index in 0..12 {
        workspace.write(
            &format!("src/f{index:02}.txt"),
            "it's a needle\n".repeat(30),
        );
    }
    let input = serde_json::json!({"queries":[{"path":"src","matchString":["needle"]}],"responseChars":1800});
    let output = workspace
        .cli()
        .env("OCTOCODE_STORAGE_MODE", "persistent")
        .args(["localSearch", &input.to_string()])
        .output()
        .expect("search");
    assert!(output.status.success(), "{}", stderr(&output));
    let commands: Vec<_> = stderr(&output)
        .lines()
        .filter_map(|line| {
            let (_, call) = line.split_once("octocode localSearch ")?;
            let input = call.strip_prefix('\'')?.strip_suffix('\'')?;
            serde_json::from_str::<Value>(&input.replace(r"'\''", "'")).ok()
        })
        .collect();
    assert!(!commands.is_empty(), "{}", stderr(&output));
    for input in commands {
        assert_eq!(input.as_object().expect("cursor input").len(), 1);
        assert!(input["cursor"].is_string());
        let resumed = workspace
            .cli()
            .env("OCTOCODE_STORAGE_MODE", "persistent")
            .args(["localSearch", &input.to_string(), "--json"])
            .output()
            .expect("resume");
        assert!(resumed.status.success(), "{}", stdout(&resumed));
        let body = compact_line(stdout(&resumed));
        assert!(body["results"].is_array());
    }
}

#[test]
fn tool_flags_are_rejected_instead_of_becoming_query_aliases() {
    let workspace = Workspace::new();
    for args in [
        vec!["localSearch", "needle", "."],
        vec!["localSearch", "--path", "."],
        vec!["localSearch", "--wholeWord", "false"],
    ] {
        let output = workspace.cli().args(args).output().expect("run");
        assert_eq!(output.status.code(), Some(2));
    }
}

/// `skill --json` answers a pipe like every other JSON command.
#[test]
fn piped_skill_json_is_one_compact_line() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("PATH", workspace.home.join("no-executables"))
        .args(["skill", "list", "--json"])
        .output()
        .expect("skill list");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let catalog = compact_line(stdout(&output));
    assert!(catalog["skills"].is_array(), "{catalog}");
}

/// A skill error is the `{error, errorCode}` envelope every command uses.
#[test]
fn skill_errors_use_the_common_envelope() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("PATH", workspace.home.join("no-executables"))
        .args(["skill", "info", "no-such-skill", "--json"])
        .output()
        .expect("skill info");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let envelope = compact_line(stdout(&output));
    let keys: Vec<&str> = envelope
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["error", "errorCode"], "{envelope}");
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert!(
        envelope["error"]
            .as_str()
            .is_some_and(|error| error.contains("no-such-skill")),
        "{envelope}"
    );
}

/// A JSON argument error keeps every line clap wrote: the error line and
/// its did-you-mean tip are `error`, the rest (possible values, usage) are
/// `details`.
#[test]
fn json_argument_errors_keep_the_whole_clap_message() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["schema", "localSearch", "--view", "notAView", "--json"])
        .output()
        .expect("schema view");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let envelope = compact_line(stdout(&output));
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert_eq!(
        envelope["error"],
        "invalid value 'notAView' for '--view <VIEW>'"
    );
    let details: Vec<&str> = envelope["details"]
        .as_array()
        .expect("details")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        details
            .iter()
            .any(|line| line.starts_with("[possible values: query, ")),
        "{envelope}"
    );
    assert!(
        details.iter().any(|line| line.contains("--help")),
        "{envelope}"
    );

    let mistyped = workspace
        .cli()
        .args(["localSerch", "--json"])
        .output()
        .expect("mistyped tool");
    let envelope = compact_line(stdout(&mistyped));
    let error = envelope["error"].as_str().expect("error");
    assert!(
        error.starts_with("unrecognized subcommand 'localSerch'; tip: ")
            && error.contains("'localSearch'"),
        "{envelope}"
    );
    let details = envelope["details"].to_string();
    assert!(details.contains("Usage: octocode"), "{envelope}");
    assert!(details.contains("--help"), "{envelope}");
}

#[cfg(unix)]
mod terminal;

const BUILD_APP: &str = r#"{"queries":[{"path":"app"}]}"#;

/// `graphBuild` reports each stage on stderr as it goes; stdout stays the
/// one JSON result, so a pipe reads it unchanged.
#[test]
fn graph_build_reports_progress_on_stderr_and_the_result_on_stdout() {
    let workspace = Workspace::new();
    workspace.write(".git/HEAD", "ref: refs/heads/main\n");
    workspace.write(
        "app/lib.ts",
        "export function f() { return g(); }\nfunction g() {}\n",
    );
    workspace.write(
        "app/main.ts",
        "import { f } from './lib';\nexport const main = () => f();\n",
    );
    let output = workspace
        .cli()
        .args(["graphBuild", "--json", BUILD_APP])
        .output()
        .expect("graphBuild");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let receipt = compact_line(stdout(&output));
    let id = receipt
        .pointer("/results/0/data/id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("snapshot id: {receipt}"));
    let lines = stderr(&output).lines().collect::<Vec<_>>();
    assert!(
        lines.iter().all(|line| line.starts_with("graphBuild: ")),
        "{lines:?}"
    );
    // Progress names the scope like every output path: workspace-relative.
    assert!(lines.contains(&"graphBuild: scanning app"), "{lines:?}");
    let mut after = 0;
    for stage in [
        "scanning ",
        "parsing 2 files",
        "parsed 2/2 files",
        "linked 2 files",
        "encoding snapshot",
        "published ",
    ] {
        let at = lines[after..]
            .iter()
            .position(|line| line.contains(stage))
            .unwrap_or_else(|| panic!("stage {stage:?} after line {after} in {lines:?}"));
        after += at + 1;
    }
    assert!(
        lines.last().is_some_and(|line| line.contains(id)),
        "{lines:?}"
    );
    let again = workspace
        .cli()
        .args(["graphBuild", "--json", BUILD_APP])
        .output()
        .expect("graphBuild again");
    let again_result = compact_line(stdout(&again));
    assert_eq!(
        again_result.pointer("/results/0/data/reused"),
        Some(&Value::Bool(true)),
        "{again_result}"
    );
    assert!(stderr(&again).contains("reused "), "{}", stderr(&again));
}
