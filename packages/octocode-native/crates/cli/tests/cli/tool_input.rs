use super::*;

#[test]
fn tool_rejects_non_canonical_fields() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args([
            "astSearch",
            r#"{"queries":[{"operation":"syntax","path":"."}]}"#,
        ])
        .output()
        .expect("astSearch");
    assert_eq!(output.status.code(), Some(2));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    // tool rejects invalid operation value — error contains field name or rejection key
    assert!(
        combined.contains("invalidInput")
            || combined.to_lowercase().contains("unexpected")
            || combined.contains("operation"),
        "{combined}"
    );
}

#[test]
fn localfetch_pages_expose_a_rerunnable_continuation() {
    let workspace = Workspace::new();
    let content: String = (1..24).map(|n| format!("line {n}: research\n")).collect();
    let path = workspace.write("source.txt", &content);
    // Line pages carry their source numbers (`<line>\t<text>`).
    let numbered: String = (1..24)
        .map(|n| format!("{n}\tline {n}: research\n"))
        .collect();
    let query = serde_json::json!({"queries":[{
        "path": path,
        "unit": "lines",
        "length": 3,
        "mainGoal": "test", "reasoning": "Verify paginated native reads."
    }]})
    .to_string();
    let first = workspace
        .cli()
        .args(["localFetch", &query, "--json"])
        .output()
        .expect("first read");
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    let value: serde_json::Value = serde_json::from_str(stdout(&first)).expect("page JSON");
    let first_content = value["results"][0]["data"]["content"]
        .as_str()
        .expect("first page content")
        .to_owned();
    assert!(numbered.starts_with(&first_content), "first page prefix");
    // Cursor tokens are per-process, so the response advertises the prefilled
    // continuation query as a directly re-runnable call.
    let call = &value["results"][0]["data"]["next"]["continueWindow"];
    assert_eq!(call["tool"], "localFetch", "{value}");
    let continuation = serde_json::to_string(&call["query"]).expect("continuation query");
    let second = workspace
        .cli()
        .args(["localFetch", &continuation, "--json"])
        .output()
        .expect("continuation read");
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    let value: serde_json::Value = serde_json::from_str(stdout(&second)).expect("page two JSON");
    let second_content = value["results"][0]["data"]["content"]
        .as_str()
        .expect("second page content");
    let joined = format!("{first_content}{second_content}");
    assert!(numbered.starts_with(&joined), "joined prefix");
    assert!(joined.len() > first_content.len(), "second page advanced");
}

#[test]
fn tool_without_query_exits_two_with_schema_hint() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["astSearch", "--json"])
        .output()
        .expect("astSearch no args");
    assert_eq!(exit_code(&output), Some(2));
    // `--json`: the usage error is the envelope on stdout.
    let error: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
    assert_bare_error(&error);
    assert_eq!(error["errorCode"], "invalidInput", "{error}");
    let message = error["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("Missing query") && message.contains("octocode astSearch --help"),
        "expected usage + help hint: {message}"
    );
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn tool_with_bad_json_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["localSearch", "{not-valid-json"])
        .output()
        .expect("localSearch bad json");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(text.contains("Invalid JSON"), "expected json error: {text}");
}

#[test]
fn tool_reads_query_from_stdin() {
    use std::io::Write;
    let workspace = Workspace::new();
    let source = workspace.write("stdin-source.rs", "fn from_stdin() {}\n");
    let query = serde_json::json!({"queries":[{"path": source}]}).to_string();
    let mut child = workspace
        .cli()
        .args(["localFetch", "--input", "-", "--json"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("localFetch --input -");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(query.as_bytes())
        .expect("write query");
    let output = child.wait_with_output().expect("output");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("from_stdin"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn tool_reads_query_from_input_file() {
    let workspace = Workspace::new();
    let source = workspace.write("input-source.rs", "fn from_file() {}\n");
    let query = serde_json::json!({"queries":[{
        "path": source,
        "mainGoal": "test", "reasoning": "Verify --input file queries."
    }]})
    .to_string();
    let query_file = workspace.write("query.json", &query);
    let output = workspace
        .cli()
        .args([
            "localFetch",
            "--input",
            query_file.to_str().expect("utf8"),
            "--json",
        ])
        .output()
        .expect("localFetch --input");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("tool JSON");
    assert!(
        value["results"][0]["data"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("from_file"))
    );
}

#[test]
fn tool_accepts_bulk_queries() {
    let workspace = Workspace::new();
    let first = workspace.write("query-one.rs", "fn query_one() {}\n");
    let second = workspace.write("query-two.rs", "fn query_two() {}\n");
    let query = serde_json::json!({"queries":[
        {
            "path": first,
            "ranges": ["1-1"],
            "mainGoal": "test", "reasoning": "Verify the first native bulk query."
        },
        {
            "path": second,
            "ranges": ["1-1"],
            "mainGoal": "test", "reasoning": "Verify the second native bulk query."
        }
    ]})
    .to_string();
    let output = workspace
        .cli()
        .args(["localFetch", &query, "--json"])
        .output()
        .expect("bulk queries");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool JSON");
    let rows = value["results"].as_array().expect("bulk rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[1]["index"], 1);
    assert!(
        rows[0]["data"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("query_one"))
    );
    assert!(
        rows[1]["data"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("query_two"))
    );
}

/// Host spellings of line ranges read the same lines as the canonical form.
/// Admission accepts only the canonical `start-end` range spelling; a host
/// spelling is the caller's input to fix (exit 2), never repaired.
#[test]
fn localfetch_rejects_host_line_range_spellings() {
    let workspace = Workspace::new();
    let lines: String = (1..=10).map(|n| format!("line {n}\n")).collect();
    let path = workspace.write("ranges.txt", &lines);
    for ranges in [
        serde_json::json!("2,4"),
        serde_json::json!([" 2-4"]),
        serde_json::json!(["2", "4"]),
        serde_json::json!([2, 4]),
    ] {
        let query = serde_json::json!({"queries":[{
            "path": path, "ranges": ranges,
            "mainGoal": "test", "reasoning": "Verify strict line ranges."
        }]})
        .to_string();
        let output = workspace
            .cli()
            .args(["localFetch", &query, "--json"])
            .output()
            .expect("ranges query");
        assert_eq!(exit_code(&output), Some(2), "{ranges}: {}", stdout(&output));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool JSON");
        assert!(
            value.to_string().contains("ranges"),
            "{ranges}: the error names the field: {value}"
        );
    }
}

/// A JSON query given as an argument is the whole input: the CLI never reads
/// or waits on stdin, so a caller that keeps stdin open (a pipe from a
/// long-running producer, an agent harness) does not delay the call.
#[test]
fn inline_json_query_never_waits_on_an_open_stdin() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let workspace = Workspace::new();
    workspace.write("note.txt", "alpha\nbeta\n");
    let query = serde_json::json!({"queries": [{"path": "note.txt", "ranges": ["1-1"]}]});
    let mut child = workspace
        .cli()
        .args(["localFetch", &query.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    // Hold the write end open: a read of stdin would block until it closes.
    let _held_open = child.stdin.take();
    let started = Instant::now();
    let deadline = started + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        let waited = Instant::now() > deadline;
        if waited {
            let _ = child.kill();
        }
        assert!(
            !waited,
            "the CLI waited on stdin although the query was an argument"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "{status}");
}
