//! What the CLI prints, by stream: JSON is indented for a terminal and one
//! compact line on a pipe; errors keep their whole message and one envelope;
//! a pipe-implied JSON error is also told to a person watching stderr.
//!
//! A terminal is a pseudo-terminal from `openpty`, given to the child as its
//! stdout or stderr. The terminal turns `\n` into `\r\n`; [`tty`] undoes it.

use crate::support::Workspace;
use octocode_test_support::json_fixture;
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
    let query = json_fixture(
        r#"{
            "queries": [
                {
                    "path": ".",
                    "matchString": ["needle"],
                    "contextLines": {"before": 1, "after": 1}
                }
            ]
        }"#,
    );
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
    let files = json_fixture(
        r#"{
            "queries": [{"path": ".", "matchString": ["needle"], "view": "files"}]
        }"#,
    );
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
    let input = json_fixture(
        r#"{
            "queries": [{"path": "src", "matchString": ["needle"]}],
            "responseChars": 1800
        }"#,
    );
    // Persistent storage starts a warm server: the fixture owns its socket
    // namespace and stops it when the test ends.
    let persistent = workspace.persistent_cli();
    let output = persistent
        .cli()
        .args(["localSearch", &input.to_string()])
        .output()
        .expect("search");
    assert!(output.status.success(), "{}", stderr(&output));
    // The continuation is on stdout, after the hits it continues.
    let commands: Vec<_> = stdout(&output)
        .lines()
        .filter_map(|line| {
            let (_, call) = line.split_once("octocode localSearch ")?;
            let input = call.strip_prefix('\'')?.strip_suffix('\'')?;
            serde_json::from_str::<Value>(&input.replace(r"'\''", "'")).ok()
        })
        .collect();
    assert!(!commands.is_empty(), "{}", stdout(&output));
    for input in commands {
        assert_eq!(input.as_object().expect("cursor input").len(), 1);
        assert!(input["cursor"].is_string());
        let resumed = persistent
            .cli()
            .args(["localSearch", &input.to_string(), "--json"])
            .output()
            .expect("resume");
        assert!(resumed.status.success(), "{}", stdout(&resumed));
        let body = compact_line(stdout(&resumed));
        assert!(body["results"].is_array());
    }
}

/// The command a text page prints for `label` (`next.nextPage`), parsed back
/// to its JSON input.
fn printed_call(text: &str, label: &str) -> Value {
    let line = text
        .lines()
        .find(|line| line.starts_with(&format!("# {label}: ")))
        .unwrap_or_else(|| panic!("{label} in {text}"));
    let (_, call) = line.split_once("octocode localSearch ").expect("command");
    let input = call
        .strip_prefix('\'')
        .and_then(|call| call.strip_suffix('\''))
        .expect("quoted input");
    serde_json::from_str(&input.replace(r"'\''", "'")).expect("JSON input")
}

/// A continuation page prints exactly as a first page does: the request its
/// cursor continues decides the line format, with context lines or without,
/// counters on stderr.
#[test]
fn a_continuation_page_prints_in_its_first_pages_line_format() {
    let workspace = Workspace::new();
    let source: String = (1..=6)
        .map(|n| format!("before {n}\nneedle {n}\n"))
        .collect();
    workspace.write("src/notes.txt", &source);
    // Persistent storage starts a warm server: the fixture owns its socket
    // namespace and stops it when the test ends.
    let persistent = workspace.persistent_cli();
    let page = |input: &Value| {
        let output = persistent
            .cli()
            .args(["localSearch", &input.to_string()])
            .output()
            .expect("search");
        assert!(output.status.success(), "{}", stdout(&output));
        (stdout(&output).to_owned(), stderr(&output).to_owned())
    };
    for (context, expected) in [
        (serde_json::json!(null), "6:needle 3\n8:needle 4\n"),
        (
            json_fixture(r#"{"before": 1, "after": 0}"#),
            "5-before 3\n6:needle 3\n7-before 4\n8:needle 4\n",
        ),
    ] {
        let mut row = json_fixture(r#"{"path": "src", "matchString": ["needle"], "limit": 2}"#);
        if !context.is_null() {
            row["contextLines"] = context;
        }
        let (first, first_notes) = page(&serde_json::json!({"queries":[row]}));
        assert!(first.contains("2:needle 1\n"), "{first}");
        // A page 1 that stopped early states no totals (design-l13: counts are
        // absent while more pages remain); the counters it does state are scanned.
        assert!(!first_notes.contains("matchCount"), "{first_notes}");
        assert!(first_notes.contains("# filesScanned"), "{first_notes}");
        let (second, second_notes) = page(&printed_call(&first, "next.nextPage"));
        assert!(second.starts_with("src/notes.txt"), "{second}");
        assert!(second.contains(expected), "{second}");
        assert!(second.contains("\n# pagination: "), "{second}");
        assert!(second.contains("\n# next.nextPage: "), "{second}");
        assert!(
            !second.contains("value:") && !second.contains("matchCount"),
            "{second}"
        );
        assert!(!second_notes.contains("matchCount"), "{second_notes}");
        assert!(second_notes.contains("# filesScanned"), "{second_notes}");
    }
}

/// A call the runtime rejects before it runs states its outcome on stdout as
/// a failed row does: its fields, then `# status: error`. Stderr holds only
/// `# ` notes, and the exit code is the rejection's.
#[test]
fn a_rejected_call_states_its_outcome_on_stdout() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args([
            "localSearch",
            r#"{"queries":[{"path":".","matchStrin":["x"]}]}"#,
        ])
        .output()
        .expect("rejected");
    assert_eq!(output.status.code(), Some(2));
    let text = stdout(&output);
    assert!(
        text.starts_with("errorCode: invalidInput\nerror: Unknown field 'matchStrin'"),
        "{text}"
    );
    assert!(text.ends_with("# status: error\n"), "{text}");
    for note in stderr(&output).lines() {
        assert!(note.starts_with("# "), "an unprefixed stderr line: {note}");
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

/// A skill error is the `{error, errorCode, errorClass}` envelope every command uses.
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
    assert_eq!(keys, ["error", "errorCode", "errorClass"], "{envelope}");
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert_eq!(envelope["errorClass"], "correctInput");
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

/// A served call is its `tool` and `input`: JSON states no `kind` on any
/// call, and text still prints each call by the channel that holds it, a
/// `related[]` lead as a stderr note and never as a stdout continuation.
#[test]
fn served_calls_state_no_kind_and_text_prints_leads_by_their_channel() {
    let workspace = Workspace::new();
    workspace.write("src/a.txt", "needle\n");
    let input = r#"{"queries":[{"path":"src/missing","matchString":["needle"]}]}"#;
    let json = workspace
        .cli()
        .args(["localSearch", input, "--json"])
        .output()
        .expect("json");
    let body = compact_line(stdout(&json));
    fn kinds(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(fields) => {
                if fields.contains_key("tool") && fields.contains_key("kind") {
                    found.push(value.clone());
                }
                fields.values().for_each(|child| kinds(child, found));
            }
            Value::Array(items) => items.iter().for_each(|child| kinds(child, found)),
            _ => {}
        }
    }
    let mut stated = Vec::new();
    kinds(&body, &mut stated);
    assert!(stated.is_empty(), "{body}");
    let lead = &body["results"][0]["data"]["related"][0];
    assert_eq!(lead["tool"], "structureSearch", "{body}");
    assert!(lead["input"]["queries"].is_array(), "{body}");

    let text = workspace
        .cli()
        .args(["localSearch", input])
        .output()
        .expect("text");
    let notes = stderr(&text);
    let printed = notes
        .lines()
        .find_map(|line| line.strip_prefix("# related[0]: octocode structureSearch "))
        .unwrap_or_else(|| panic!("a related[0] lead note: {notes}"));
    let printed: Value = serde_json::from_str(
        &printed
            .strip_prefix('\'')
            .and_then(|call| call.strip_suffix('\''))
            .expect("quoted input")
            .replace(r"'\''", "'"),
    )
    .expect("JSON input");
    assert_eq!(printed, lead["input"]);
    assert!(!stdout(&text).contains("related[0]"), "{}", stdout(&text));
}

#[cfg(unix)]
mod terminal;
