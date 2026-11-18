use super::output::Printed;
use super::parse::parse_args_from;
use super::{Command, OutputMode, dispatch, lsp_provision, start_failed};
use octocode_contract_types::ErrorCode;
use octocode_native::runtime::RuntimeError;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use octocode_test_support::json_fixture;
use serde_json::json;

/// A terminal gets indented JSON, a pipe one compact line, whatever the
/// command; both are the same value.
#[test]
fn json_is_indented_for_a_terminal_and_compact_for_a_pipe() {
    let value = json_fixture(
        r#"{
            "results": [{"index": 0, "data": {"a": [1, 2]}}],
            "root": "/w"
        }"#,
    );
    let pipe = super::output::json_text(&value, false).expect("compact");
    assert_eq!(
        pipe,
        r#"{"results":[{"index":0,"data":{"a":[1,2]}}],"root":"/w"}"#
    );
    assert_eq!(pipe, value.to_string());
    let terminal = super::output::json_text(&value, true).expect("pretty");
    assert!(terminal.starts_with("{\n  \"results\": [\n"), "{terminal}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&terminal).ok(),
        Some(value)
    );
}

/// A clap error keeps every line in JSON: the first is `error`, the rest
/// (possible values, tips, usage) are `details`.
#[test]
fn clap_errors_keep_every_line_in_json() {
    use clap::CommandFactory;
    let error = super::Args::command()
        .try_get_matches_from(["octocode", "schema", "localSearch", "--view", "notAView"])
        .expect_err("invalid view");
    let envelope = super::parse::clap_error_envelope(&error);
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert_eq!(
        envelope["error"],
        "invalid value 'notAView' for '--view <VIEW>'"
    );
    let details = envelope["details"].as_array().expect("details");
    assert!(
        details[0]
            .as_str()
            .is_some_and(|line| line.starts_with("[possible values: query, ")),
        "{envelope}"
    );
}

/// A JSON error carries its text through the warm server, so a client
/// can tell a watching stderr; a result or a text error carries none.
#[cfg(unix)]
#[test]
fn printed_calls_round_trip_through_the_reply_line() {
    let error = || RuntimeError {
        code: ErrorCode::Timeout,
        message: "Tool call timed out.".into(),
        payload: None,
        validation_issues: None,
    };
    let json_error = Printed::finish("localSearch", Err(error()), true, &json!({}), None);
    assert!(
        json_error
            .error_text
            .as_deref()
            .is_some_and(|text| text.ends_with("(errorCode: timeout)"))
    );
    let text_error = Printed::finish("localSearch", Err(error()), false, &json!({}), None);
    assert_eq!(text_error.error_text, None);
    for printed in [json_error, text_error] {
        assert_eq!(
            Printed::from_json(printed.to_json()).as_ref(),
            Some(&printed)
        );
    }
}

/// After the first Ctrl-C cancels, a second Ctrl-C must stop waiting
/// for work that ignores cancellation; without one, the drain completes.
#[tokio::test]
async fn second_interrupt_stops_waiting_for_uncooperative_work() {
    use super::tool_call::{Interrupted, drain_after_interrupt};
    let forced = drain_after_interrupt(std::future::pending::<()>(), std::future::ready(())).await;
    assert_eq!(forced, Interrupted::Forced);
    let drained = drain_after_interrupt(std::future::ready(()), std::future::pending::<()>()).await;
    assert_eq!(drained, Interrupted::Drained);
}

/// A rejected call prints the runtime's one projection on stdout: the typed
/// envelope in JSON mode, the text of a failed row otherwise (its fields,
/// then `# status: error`), and exits by the code's class.
#[test]
fn rejected_calls_print_the_runtime_projection() {
    use octocode_runtime_base::contracts;
    let tool = "localFetch";
    let error = contracts::validate(
        tool,
        json_fixture(r#"{"queries": [{"path": "a.rs", "matchstring": "x"}]}"#),
    )
    .expect_err("invalid input");
    let invalid = || RuntimeError {
        code: ErrorCode::InvalidInput,
        message: error.to_string(),
        payload: Some(Box::new(contracts::format_input_error(
            tool,
            &error,
            &[],
            &serde_json::Value::Null,
        ))),
        validation_issues: None,
    };
    let json_out = Printed::finish(tool, Err(invalid()), true, &json!({}), None);
    assert_eq!((json_out.exit, json_out.json), (2, true));
    let envelope: serde_json::Value = serde_json::from_str(&json_out.body).expect("json");
    assert!(envelope.get("kind").is_none(), "{envelope}");
    assert_eq!(envelope["errorCode"], "invalidInput", "{envelope}");
    let text = Printed::finish(tool, Err(invalid()), false, &json!({}), None);
    assert_eq!((text.exit, text.json), (2, false));
    assert!(
        text.body
            .starts_with("errorCode: invalidInput\nerror: Unknown field 'matchstring'"),
        "{}",
        text.body
    );
    assert!(text.body.contains("\n# status: error"), "{}", text.body);
    assert_eq!((&text.error_text, &text.notes), (&None, &None));

    let timeout = RuntimeError {
        code: ErrorCode::Timeout,
        message: "Tool call timed out.".into(),
        payload: None,
        validation_issues: None,
    };
    let expected = octocode_native::runtime::error::tool_error_result("localSearch", &timeout);
    // `timeout` is a transient code, so the CLI exits 7.
    let printed = Printed::finish("localSearch", Err(timeout), false, &json!({}), None);
    assert_eq!((printed.exit, printed.json), (7, false));
    assert_eq!(
        printed.body,
        super::lines::rejected(&expected.envelope),
        "the envelope's fields, as text"
    );
    assert!(
        printed
            .body
            .starts_with("errorCode: timeout\nerror: Tool call timed out."),
        "{}",
        printed.body
    );
}

/// A continuation's line format is the one of the request it continued:
/// the runtime names that request, and a bare cursor alone says nothing of
/// it, so the same page prints as `key: value` text without it.
#[test]
fn a_continuation_prints_in_the_line_format_of_the_request_it_continued() {
    use octocode_runtime_base::response::outcome::ToolOutcome;
    use octocode_runtime_base::security::Sanitized;
    let page = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "matchCount": 9,
                        "files": [
                            {
                                "path": "a.rs",
                                "matches": [{"line": 26, "value": "25\tuse a;\n26\tuse b;"}]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let printed = |continued: Option<serde_json::Value>| {
        Printed::finish(
            "localSearch",
            Ok(ToolOutcome {
                structured_content: Sanitized::scrubbed(page.clone()),
                failure: None,
                all_failed: false,
                continued,
            }),
            false,
            &json_fixture(r#"{"cursor": "cursor2.page"}"#),
            None,
        )
    };
    let asked = json_fixture(
        r#"{
            "queries": [
                {
                    "path": "a.rs",
                    "matchString": ["use"],
                    "contextLines": {"before": 1, "after": 0}
                }
            ]
        }"#,
    );
    let continued = printed(Some(asked));
    assert_eq!(continued.body, "a.rs\n25-use a;\n26:use b;");
    assert_eq!(continued.notes.as_deref(), Some("# matchCount 9"));
    // A page holding one row of a batch reads that row's own query, under
    // that query's heading.
    let batch = json_fixture(
        r#"{
            "queries": [
                {"path": "b.rs", "matchString": ["x"]},
                {
                    "path": "a.rs",
                    "matchString": ["use"],
                    "contextLines": {"before": 1, "after": 0}
                }
            ]
        }"#,
    );
    let mut second_row = page.clone();
    second_row["results"][0]["index"] = json!(1);
    let later = Printed::finish(
        "localSearch",
        Ok(ToolOutcome {
            structured_content: Sanitized::scrubbed(second_row),
            failure: None,
            all_failed: false,
            continued: Some(batch),
        }),
        false,
        &json_fixture(r#"{"cursor": "cursor2.page"}"#),
        None,
    );
    assert_eq!(later.body, "## results[1]\na.rs\n25-use a;\n26:use b;");
    let unknown = printed(None);
    assert!(
        unknown.body.starts_with("matchCount: 9\nfiles:"),
        "{}",
        unknown.body
    );
}

#[test]
fn every_contract_tool_parses_as_a_tool_command() {
    use clap::CommandFactory;
    super::Args::command().debug_assert();
    let contract = octocode_runtime_base::contracts::parsed_contract().expect("contract");
    for tool in contract["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("name");
        let args = parse_args_from(["octocode", name, "{}", "--json"]).expect(name);
        assert!(args.json, "{name}");
        match args.command {
            super::Command::Tool(command) => {
                assert_eq!(command.name, name);
                assert_eq!(command.args.json_arg(), Some("{}"));
            }
            _ => panic!("{name} did not parse as a tool"),
        }
    }
}

/// `--json` is one global flag: every command takes it in any position.
#[test]
fn json_is_one_global_flag() {
    for argv in [
        vec!["octocode", "--json", "config", "home"],
        vec!["octocode", "config", "get", "K", "--json"],
        vec!["octocode", "config", "--json"],
        vec!["octocode", "auth", "status", "--json"],
        vec!["octocode", "auth", "logout", "--json"],
        vec!["octocode", "install", "--list", "--json"],
        vec!["octocode", "graphQuery", "--json", "{}"],
        vec!["octocode", "lsp-server", "list", "--json"],
    ] {
        let args = parse_args_from(argv.clone()).expect("parses");
        assert!(args.json, "{argv:?}");
    }
}

#[test]
fn argument_errors_exit_two_in_either_output_mode() {
    assert_eq!(
        parse_args_from(["octocode", "notACommand", "--json"]).err(),
        Some(2)
    );
    assert_eq!(parse_args_from(["octocode", "notACommand"]).err(), Some(2));
    assert!(parse_args_from(["octocode", "schema", "localSearch", "--view", "query"]).is_ok());
    assert_eq!(
        parse_args_from(["octocode", "install", "--method", "yarn"]).err(),
        Some(2)
    );
}

#[test]
fn removed_aliases_do_not_parse() {
    for argv in [
        vec!["octocode", "scheme"],
        vec!["octocode", "showConfig"],
        vec!["octocode", "catalog"],
        vec!["octocode", "--json-errors", "schema"],
        vec!["octocode", "--redact-emails", "schema"],
        vec!["octocode", "--no-color", "schema"],
        vec!["octocode", "localSearch", "{}", "--pretty"],
        vec!["octocode", "config", "--add", "K", "V"],
        vec!["octocode", "auth", "--json"],
    ] {
        assert_eq!(parse_args_from(argv.clone()).err(), Some(2), "{argv:?}");
    }
}

#[test]
fn tool_input_reads_a_file_and_rejects_bad_json() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("q.json");
    std::fs::write(&path, r#"{"queries":[]}"#).expect("write");
    let args = super::commands::ToolArgs {
        args: Vec::new(),
        input: Some(path),
        content_stdin: false,
    };
    assert_eq!(
        super::tool_call::tool_input("localSearch", &args),
        Ok(json_fixture(r#"{"queries": []}"#))
    );
    let bad = super::commands::ToolArgs {
        args: vec!["{".into()],
        input: None,
        content_stdin: false,
    };
    assert!(
        super::tool_call::tool_input("localSearch", &bad)
            .is_err_and(|e| e.starts_with("Invalid JSON"))
    );
    let missing = super::commands::ToolArgs {
        args: Vec::new(),
        input: None,
        content_stdin: false,
    };
    assert!(
        super::tool_call::tool_input("localSearch", &missing)
            .is_err_and(|e| e.starts_with("Missing query"))
    );
}

/// `clasify --content -`: stdin becomes the one row's `content`, byte for
/// byte; a batch, a cursor, or a row that names its own input is refused
/// with the conflict named, and stdin cannot carry both the JSON and the text.
#[test]
fn content_from_stdin_fills_exactly_one_row_that_names_no_input() {
    use super::tool_call::{stdin_text, tool_input, with_content};
    let row = json_fixture(
        r#"{
            "operation": "evaluate",
            "questions": {"q": {"type": "noul", "instructions": "Does a test fail?"}}
        }"#,
    );
    let text = "test a ... ok\ntest b ... FAILED\n\u{e9}\n".to_owned();
    let filled =
        with_content("clasify", json!({"queries":[row.clone()]}), text.clone()).expect("one row");
    assert_eq!(filled["queries"][0]["content"], json!(text));
    assert_eq!(filled["queries"][0]["questions"], row["questions"]);
    let error = |query: serde_json::Value| {
        with_content("clasify", query, String::new()).expect_err("refused")
    };
    assert!(error(json_fixture(r#"{"cursor": "c"}"#)).contains("no queries"));
    assert!(error(json!({"queries":[row.clone(), row.clone()]})).contains("has 2 rows"));
    assert!(error(json_fixture(r#"{"queries": []}"#)).contains("has 0 rows"));
    for field in ["content", "source", "resources"] {
        let mut named = row.clone();
        named[field] = json!("x");
        let message = error(json!({"queries":[named]}));
        assert!(
            message.contains(&format!("drop its `{field}`")),
            "{message}"
        );
    }
    assert_eq!(stdin_text(text.as_bytes()), Ok(text));
    assert!(stdin_text(&[0xff, 0xfe][..]).is_err_and(|e| e.contains("not UTF-8")));
    let both = super::commands::ToolArgs {
        args: Vec::new(),
        input: Some("-".into()),
        content_stdin: true,
    };
    assert!(tool_input("clasify", &both).is_err_and(|e| e.contains("both read stdin")));
}

/// The query's own size bound holds once stdin is in it.
#[test]
fn content_from_stdin_keeps_the_query_size_bound() {
    let row = json_fixture(
        r#"{
            "operation": "evaluate",
            "questions": {"q": {"type": "noul", "instructions": "Q?"}}
        }"#,
    );
    let limit = usize::try_from(super::commands::MAX_QUERY_BYTES).expect("usize");
    let refused =
        super::tool_call::with_content("clasify", json!({"queries":[row]}), "a".repeat(limit));
    assert!(refused.is_err_and(|e| e.contains("exceeds 8 MiB")));
}

/// Only clasify takes `--content`, and only `-`; it combines with a JSON
/// argument or `--input FILE`.
#[test]
fn only_clasify_takes_content_from_stdin() {
    use clap::CommandFactory;
    let parse = |argv: &[&str]| super::Args::command().try_get_matches_from(argv);
    let json = r#"{"queries":[{}]}"#;
    assert!(parse(&["octocode", "clasify", "--content", "-", json]).is_ok());
    assert!(parse(&["octocode", "clasify", "--content", "-", "--input", "q.json"]).is_ok());
    assert!(parse(&["octocode", "clasify", "--content", "file.txt", json]).is_err());
    assert!(parse(&["octocode", "localSearch", "--content", "-", json]).is_err());
    let Ok(super::Args {
        command: Command::Tool(tool),
        ..
    }) = parse_args_from(["octocode", "clasify", "--content", "-", json])
    else {
        panic!("clasify call");
    };
    assert!(tool.args.content_stdin);
    let Ok(super::Args {
        command: Command::Tool(tool),
        ..
    }) = parse_args_from(["octocode", "clasify", json])
    else {
        panic!("clasify call");
    };
    assert!(!tool.args.content_stdin);
}

/// B75: no surface sets a request budget, so a cold language-server request
/// (`initialize`, readiness, the content-modified retries, then its own
/// timeout) always reaches its base; only a caller's own budget can end it
/// sooner.
#[test]
fn no_surface_budget_cuts_a_cold_lsp_request() {
    assert_eq!(
        octocode_runtime_base::execution::RequestLimits::default().timeout,
        None
    );
}

/// A runtime in a fresh home; its GitHub host is not github.com, so no
/// command asks a credential store for a token.
fn fixture_runtime(home: &std::path::Path) -> ToolRuntime {
    let env = [
        ("OCTOCODE_HOME", home.to_string_lossy().into_owned()),
        ("GITHUB_API_URL", "https://ghe.example/api/v3".to_owned()),
    ]
    .map(|(key, value)| (key.to_owned(), value));
    ToolRuntime::from_host(HostOptions {
        cwd: Some(home.to_path_buf()),
        env: Some(env.into_iter().collect()),
        ..HostOptions::default()
    })
    .expect("fixture runtime")
}

/// Commands that need a runtime reach their owner through `dispatch`; one
/// that never reaches it (a tool, `serve`, `mcp`) is a usage error there,
/// and so is a schema request for a tool that does not exist.
#[tokio::test(flavor = "current_thread")]
async fn dispatch_routes_runtime_commands_to_their_owner() {
    let dir = tempfile::tempdir().expect("home");
    let runtime = fixture_runtime(dir.path());
    for json in [false, true] {
        let mode = OutputMode::from(json);
        assert_eq!(dispatch(Command::Mcp, &runtime, mode).await, 2);
        assert_eq!(
            dispatch(
                Command::LspServer {
                    command: lsp_provision::LspServerCommand::List
                },
                &runtime,
                mode
            )
            .await,
            0
        );
        assert_eq!(
            dispatch(
                Command::Schema {
                    tool: Some("noSuchTool".into()),
                    view: None,
                    select: None
                },
                &runtime,
                mode
            )
            .await,
            2
        );
        assert_eq!(
            dispatch(
                Command::Schema {
                    tool: Some("localSearch".into()),
                    view: Some("variants".into()),
                    select: None
                },
                &runtime,
                mode
            )
            .await,
            0
        );
    }
}

/// A runtime that cannot start exits by its error's class in either output.
#[test]
fn a_runtime_that_cannot_start_exits_by_its_error_class() {
    for json in [false, true] {
        let error = RuntimeError {
            code: ErrorCode::InvalidInput,
            message: "bad host options".into(),
            payload: None,
            validation_issues: None,
        };
        assert_eq!(start_failed(&error, OutputMode::from(json)), 2);
    }
}

/// The request fields a tool's input schema names: its own `properties`
/// and those of each form it may take (`anyOf`, `oneOf`, `allOf`), never a
/// row's.
fn request_fields(schema: &serde_json::Value, fields: &mut std::collections::BTreeSet<String>) {
    fields.extend(
        schema["properties"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(field, _)| field.clone()),
    );
    for forms in ["anyOf", "oneOf", "allOf"] {
        for form in schema[forms].as_array().into_iter().flatten() {
            request_fields(form, fields);
        }
    }
}

/// The root help names every request field the tool contracts take beside
/// the rows, as it names `queries`: none is learned only from a tool's own
/// help.
#[test]
fn root_help_names_every_request_field_of_the_contract() {
    let mut fields = std::collections::BTreeSet::new();
    for tool in octocode_runtime_base::id::ToolId::ALL {
        let contract = octocode_runtime_base::contracts::tool_contract_named(tool.as_str())
            .expect("a contract tool")
            .expect("its contract");
        request_fields(&contract["inputSchema"], &mut fields);
    }
    assert!(fields.contains("queries"), "{fields:?}");
    for field in &fields {
        assert!(
            super::ROOT_HELP_HEAD.contains(&format!("\"{field}\"")),
            "{field} unnamed in: {}",
            super::ROOT_HELP_HEAD
        );
    }
}
