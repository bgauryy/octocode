use super::Printed;
use super::output::Stream;
use super::parse::parse_args_from;
use octocode_contract_types::ErrorCode;
use octocode_native::runtime::RuntimeError;
use serde_json::json;

/// A terminal gets indented JSON, a pipe one compact line, whatever the
/// command; both are the same value.
#[test]
fn json_is_indented_for_a_terminal_and_compact_for_a_pipe() {
    let value = json!({"results": [{"index": 0, "data": {"a": [1, 2]}}], "root": "/w"});
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
        .try_get_matches_from(["octocode", "graph", "query", "notAnOp"])
        .expect_err("invalid op");
    let envelope = super::parse::clap_error_envelope(&error);
    assert_eq!(envelope["errorCode"], "invalidInput");
    assert_eq!(envelope["error"], "invalid value 'notAnOp' for '<OP>'");
    let details = envelope["details"].as_array().expect("details");
    assert!(
        details[0]
            .as_str()
            .is_some_and(|line| line.starts_with("[possible values: stats, ")),
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
    use super::{Interrupted, drain_after_interrupt};
    let forced = drain_after_interrupt(std::future::pending::<()>(), std::future::ready(())).await;
    assert_eq!(forced, Interrupted::Forced);
    let drained = drain_after_interrupt(std::future::ready(()), std::future::pending::<()>()).await;
    assert_eq!(drained, Interrupted::Drained);
}

/// A rejected call prints the runtime's one projection: the typed
/// envelope on stdout in JSON mode, its text on stderr otherwise, and
/// exits by the code's class.
#[test]
fn rejected_calls_print_the_runtime_projection() {
    use octocode_native::contracts;
    let tool = "localFetch";
    let error = contracts::prepare_many_and_validate(
        tool,
        json!({"queries":[{"path":"a.rs","matchstring":"x"}]}),
    )
    .expect_err("invalid input");
    let invalid = || RuntimeError {
        code: ErrorCode::InvalidInput,
        message: error.to_string(),
        payload: Some(Box::new(contracts::format_input_error(tool, &error))),
        validation_issues: None,
    };
    let json_out = Printed::finish(tool, Err(invalid()), true, &json!({}), None);
    assert_eq!((json_out.exit, json_out.stream), (2, Stream::Stdout));
    let envelope: serde_json::Value = serde_json::from_str(&json_out.body).expect("json");
    assert!(envelope.get("kind").is_none(), "{envelope}");
    assert_eq!(envelope["errorCode"], "invalidInput", "{envelope}");
    let text = Printed::finish(tool, Err(invalid()), false, &json!({}), None);
    assert_eq!((text.exit, text.stream), (2, Stream::Stderr));
    assert!(
        text.body.contains("Unknown field 'matchstring'"),
        "{}",
        text.body
    );
    assert!(
        text.body.ends_with("(errorCode: invalidInput)"),
        "{}",
        text.body
    );

    let timeout = RuntimeError {
        code: ErrorCode::Timeout,
        message: "Tool call timed out.".into(),
        payload: None,
        validation_issues: None,
    };
    let printed = Printed::finish("localSearch", Err(timeout), false, &json!({}), None);
    assert_eq!(printed.exit, 5);
    assert!(
        printed.body.contains("Retry, or narrow"),
        "{}",
        printed.body
    );
}

#[test]
fn every_contract_tool_parses_as_a_tool_command() {
    use clap::CommandFactory;
    super::Args::command().debug_assert();
    let contract = octocode_native::contracts::parsed_contract().expect("contract");
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
        vec!["octocode", "graph", "query", "stats", "--json"],
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
    let args = super::ToolArgs {
        args: Vec::new(),
        input: Some(path),
    };
    assert_eq!(
        super::tool_input("localSearch", &args),
        Ok(json!({"queries": []}))
    );
    let bad = super::ToolArgs {
        args: vec!["{".into()],
        input: None,
    };
    assert!(super::tool_input("localSearch", &bad).is_err_and(|e| e.starts_with("Invalid JSON")));
    let missing = super::ToolArgs {
        args: Vec::new(),
        input: None,
    };
    assert!(
        super::tool_input("localSearch", &missing).is_err_and(|e| e.starts_with("Missing query"))
    );
}

#[test]
fn execution_deadline_exceeds_the_worst_cold_lsp_budget() {
    assert!(
        octocode_native::execution::DEFAULT_TIMEOUT.as_millis()
            > u128::from(octocode_engine::lsp::MAX_COLD_LSP_EXECUTION_BUDGET_MS)
    );
}
