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
        r#"{"queries":[{"path":"notes.txt","matchString":"needle"}]}"#,
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

/// A tool prints text on a pipe, as rg and gh do: hits as `path:line:text`
/// on stdout, counts and leads as `# ` notes on stderr. Flags and the JSON
/// query print the same lines.
#[test]
fn tools_print_rg_style_lines_on_a_pipe() {
    let workspace = Workspace::new();
    workspace.write("notes.txt", "alpha\nneedle here\nomega\n");
    let flags = workspace
        .cli()
        .args(["localSearch", "needle", ".", "-C", "1"])
        .output()
        .expect("flags");
    assert_eq!(flags.status.code(), Some(0), "{}", stderr(&flags));
    assert_eq!(
        stdout(&flags),
        // With context, as `rg --heading`: the path once, then the lines.
        "notes.txt\n1-alpha\n2:needle here\n3-omega\n"
    );
    assert!(
        stderr(&flags).contains("# matchCount: 1"),
        "{}",
        stderr(&flags)
    );
    let json = workspace
        .cli()
        .args([
            "localSearch",
            r#"{"path":".","matchString":"needle","contextLines":1}"#,
        ])
        .output()
        .expect("json query");
    assert_eq!(stdout(&json), stdout(&flags));
    let files = workspace
        .cli()
        .args(["localSearch", "needle", "-l"])
        .output()
        .expect("files");
    assert_eq!(stdout(&files), "notes.txt\n");
}

/// Every call a JSON result offers, as `prefix|tool|query JSON`: `next.*`
/// and other `{tool, query}` objects under their path, and `hints` strings
/// (`[name: ][Approximate: ]tool {json}`) under theirs. A single row's
/// paths drop `results[0].data.`, as the text notes do.
fn json_calls(result: &Value) -> Vec<String> {
    fn walk(value: &Value, label: &str, out: &mut Vec<String>) {
        if let (Some(tool), Some(query)) = (value["tool"].as_str(), value.get("query")) {
            out.push(format!("{label}: |{tool}|{query}"));
            return;
        }
        let join = |key: &str| {
            if label.is_empty() {
                key.to_owned()
            } else {
                format!("{label}.{key}")
            }
        };
        match value {
            Value::Object(map) => map
                .iter()
                .filter(|(key, _)| {
                    !["files", "content", "matches", "value"].contains(&key.as_str())
                })
                .for_each(|(key, child)| walk(child, &join(key), out)),
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    let call =
                        item.as_str()
                            .filter(|_| label.ends_with("hints"))
                            .and_then(|text| {
                                let (head, json) = text.split_at(text.find(" {")?);
                                let (prefix, tool) =
                                    head.split_at(head.rfind(' ').map_or(0, |i| i + 1));
                                Some(format!("{label}: {prefix}|{tool}|{}", json.trim_start()))
                            });
                    match call {
                        Some(call) => out.push(call),
                        None => walk(item, &format!("{label}[{index}]"), out),
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(result, "", &mut out);
    out.into_iter()
        .map(|line| {
            line.strip_prefix("results[0].data.")
                .map_or(line.clone(), str::to_owned)
        })
        .collect()
}

/// The `# label: [prefix]octocode TOOL '<json>'` notes of a text-mode call,
/// unquoted to the same `prefix|tool|query JSON` form.
fn text_calls(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix("# ")?;
            let (prefix, command) = line.split_once("octocode ")?;
            let (tool, quoted) = command.split_once(' ')?;
            let json = quoted.strip_prefix('\'')?.strip_suffix('\'')?;
            Some(format!("{prefix}|{tool}|{}", json.replace(r"'\''", "'")))
        })
        .collect()
}

/// A text-mode continuation is the JSON one, byte for byte: a result
/// without a view prints the `--json` bytes, and a view prints every
/// `next.*` page and call hint with each query field the JSON result
/// carries (`extras` included), under the same label.
#[test]
fn text_mode_calls_are_byte_identical_to_the_json_calls() {
    let workspace = Workspace::new();
    for index in 0..12 {
        workspace.write(
            &format!("src/f{index:02}.txt"),
            "it's a needle\n".repeat(30),
        );
    }
    let queries = [
        (
            "localSearch",
            r#"{"path":"src","matchString":"needle","pageSize":2,"extras":["details","hints"]}"#,
        ),
        (
            "structureSearch",
            r#"{"operation":"files","path":"src","pageSize":3,"extras":["details"]}"#,
        ),
        (
            "localFetch",
            r#"{"path":"src/f00.txt","unit":"lines","length":5,"extras":["details","hints"]}"#,
        ),
        (
            "localSearch",
            r#"{"path":"src","matchString":"needle","include":["*.zz"],"extras":["hints"]}"#,
        ),
    ];
    for (tool, query) in queries {
        let json = workspace
            .cli()
            .args([tool, query, "--json"])
            .output()
            .expect("json");
        let text = workspace.cli().args([tool, query]).output().expect("text");
        assert_eq!(json.status.code(), text.status.code(), "{tool} {query}");
        if serde_json::from_str::<Value>(stdout(&text)).is_ok() {
            assert_eq!(stdout(&text), stdout(&json), "{tool} {query}");
            continue;
        }
        let expected = json_calls(&compact_line(stdout(&json)));
        assert!(!expected.is_empty(), "{tool} {query}: {}", stdout(&json));
        assert_eq!(
            text_calls(stderr(&text)),
            expected,
            "{tool} {query}\n{}",
            stderr(&text)
        );
    }
}

/// `--bool false` with a space is refused in the real parser, not read as
/// a path.
#[test]
fn a_spaced_boolean_value_is_refused() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["localSearch", "foo", "--wholeWord", "false"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("--wholeWord=false"),
        "{}",
        stderr(&output)
    );
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
        .args(["graph", "query", "notAnOp", "--json"])
        .output()
        .expect("graph query");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
    let envelope = compact_line(stdout(&output));
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert_eq!(envelope["error"], "invalid value 'notAnOp' for '<OP>'");
    let details: Vec<&str> = envelope["details"]
        .as_array()
        .expect("details")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        details
            .iter()
            .any(|line| line.starts_with("[possible values: stats, ")),
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
