//! A tool command takes one canonical JSON envelope, so its only flag is
//! how to supply that JSON (`--input`), beside the global `--json` that
//! prints the answer: no flag spells a schema key, so no field has a second CLI
//! name, and every schema key is reached the one way, inside the JSON. Its
//! help names exactly the fields core requires per form.
use clap::CommandFactory;
use octocode_native::id::ToolId;
use serde_json::Value;
use std::collections::BTreeSet;

/// The flags a tool command owns, apart from help.
const ENVELOPE_FLAGS: [&str; 1] = ["input"];

fn published_keys(tool: &str) -> BTreeSet<String> {
    let presentation = octocode_native::contracts::cli_presentation().expect("presentation");
    let entry = presentation["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|entry| entry["name"] == tool)
        .expect("presented");
    let mut keys = BTreeSet::new();
    let mut stack = vec![&entry.get("selection").unwrap_or(entry)["mcp"]["inputSchema"]];
    while let Some(schema) = stack.pop() {
        match schema {
            Value::Object(map) => {
                if let Some(properties) = map.get("properties").and_then(Value::as_object) {
                    keys.extend(properties.keys().cloned());
                }
                stack.extend(map.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    keys
}

#[test]
fn tool_commands_take_the_envelope_and_no_flag_names_a_field() {
    let command = super::Args::command();
    let mut failures = Vec::new();
    for tool in ToolId::ALL {
        let name = tool.as_str();
        let Some(sub) = command.find_subcommand(name) else {
            failures.push(format!("{name}: no command"));
            continue;
        };
        let flags: BTreeSet<&str> = sub
            .get_arguments()
            .filter_map(|argument| argument.get_long())
            .filter(|long| *long != "help")
            .collect();
        if flags != BTreeSet::from(ENVELOPE_FLAGS) {
            failures.push(format!("{name}: flags {flags:?}"));
        }
        let keys = published_keys(name);
        let globals = command
            .get_arguments()
            .filter_map(|argument| argument.get_long());
        for flag in flags.iter().copied().chain(globals) {
            if keys.contains(flag) {
                failures.push(format!("{name}: --{flag} spells a schema key"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn tool_help_names_exactly_the_required_fields_of_each_form() {
    let presentation = octocode_native::contracts::cli_presentation().expect("presentation");
    let mut failures = Vec::new();
    for tool in ToolId::ALL {
        let name = tool.as_str();
        let help = octocode_native::contracts::cli_tool_help(name).unwrap_or_default();
        let entry = presentation["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|entry| entry["name"] == name)
            .expect("presented");
        let keys = published_keys(name);
        for variant in entry["views"]["variants"]["variants"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let requires: Vec<&str> = variant["requires"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            for field in &requires {
                if !keys.contains(*field) {
                    failures.push(format!("{name}: requires unpublished {field}"));
                }
            }
            let listed = requires.join(", ");
            if !listed.is_empty() && !help.lines().any(|line| line.trim_end().ends_with(&listed)) {
                failures.push(format!("{name}: help lacks the required line {listed}"));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
