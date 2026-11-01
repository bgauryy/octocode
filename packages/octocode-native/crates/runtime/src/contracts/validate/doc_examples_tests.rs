//! Every tool request the docs and skills show is one both validators
//! accept: native admission, and the published input core generates (the
//! schema MCP lists, embedded through the presentation). An example agents
//! copy that either rejects teaches a call that fails.
//!
//! Requests are found three ways: an `octocode <tool> '<json>'` command, a
//! `{"tool":"<tool>","input":{…}}` call, and a fenced `json` block under a
//! heading (its own or an enclosing one) that names exactly one tool.
use super::validate_input_schema;
use crate::id::ToolId;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// One request an example shows: where, which tool, and the input.
struct Example {
    at: String,
    tool: String,
    input: Value,
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

/// The Markdown agents and people read: docs, public and developer skills,
/// and the repository guides.
fn markdown_files() -> Vec<PathBuf> {
    let root = repository_root();
    let mut files = vec![root.join("README.md"), root.join("AGENTS.md")];
    let mut dirs = vec![
        root.join("docs"),
        root.join("skills"),
        root.join("skills-dev"),
    ];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == "node_modules") {
                continue;
            }
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// The one tool a heading names, if it names exactly one.
fn heading_tool(heading: &str) -> Option<&'static str> {
    let words: Vec<&str> = heading
        .split(|character: char| !character.is_ascii_alphanumeric())
        .collect();
    let named: Vec<&'static str> = ToolId::ALL
        .iter()
        .map(|tool| tool.as_str())
        .filter(|tool| words.contains(tool))
        .collect();
    match named.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

/// A request envelope: fresh queries or a cursor.
fn is_request(value: &Value) -> bool {
    value.get("queries").is_some() || value.get("cursor").is_some()
}

fn examples_in(file: &Path, text: &str, out: &mut Vec<Example>, unattributed: &mut Vec<String>) {
    let command = regex::Regex::new(r"octocode ([A-Za-z<>]+) '(\{[^']*\})'").expect("regex");
    let name = file
        .strip_prefix(repository_root())
        .unwrap_or(file)
        .display()
        .to_string();
    // The enclosing headings, outermost first: (level, text).
    let mut headings: Vec<(usize, String)> = Vec::new();
    let mut block: Option<(usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let at = format!("{name}:{}", index + 1);
        if let Some((start, body)) = block.as_mut() {
            if line.trim_start().starts_with("```") {
                let (start, body) = (*start, std::mem::take(body));
                block = None;
                if let Ok(input) = serde_json::from_str::<Value>(&body)
                    && is_request(&input)
                {
                    let tool = headings
                        .iter()
                        .rev()
                        .find_map(|(_, text)| heading_tool(text));
                    match tool {
                        Some(tool) => out.push(Example {
                            at: format!("{name}:{start}"),
                            tool: tool.to_owned(),
                            input,
                        }),
                        None => unattributed.push(format!("{name}:{start}")),
                    }
                }
            } else {
                body.push_str(line);
                body.push('\n');
            }
            continue;
        }
        if line.starts_with('#') {
            let level = line
                .chars()
                .take_while(|character| *character == '#')
                .count();
            headings.retain(|(enclosing, _)| *enclosing < level);
            headings.push((level, line.to_owned()));
        }
        if line.trim_start().starts_with("```json") {
            block = Some((index + 2, String::new()));
            continue;
        }
        for capture in command.captures_iter(line) {
            if let Ok(input) = serde_json::from_str::<Value>(&capture[2]) {
                out.push(Example {
                    at: at.clone(),
                    tool: capture[1].to_owned(),
                    input,
                });
            }
        }
        if let Ok(call) = serde_json::from_str::<Value>(line.trim())
            && let (Some(tool), Some(input)) = (call["tool"].as_str(), call.get("input"))
            && is_request(input)
        {
            out.push(Example {
                at,
                tool: tool.to_owned(),
                input: input.clone(),
            });
        }
    }
}

fn published_input(tool: &str) -> &'static Value {
    let entry = crate::contracts::cli_presentation().expect("presentation")["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|entry| entry["name"] == tool)
        .expect("presented");
    &entry.get("selection").unwrap_or(entry)["mcp"]["inputSchema"]
}

#[test]
fn every_documented_request_is_accepted_by_native_and_the_published_schema() {
    let mut examples = Vec::new();
    let mut unattributed = Vec::new();
    for file in markdown_files() {
        let text = std::fs::read_to_string(&file).expect("markdown");
        examples_in(&file, &text, &mut examples, &mut unattributed);
    }
    assert!(
        examples.len() > 20,
        "the extractor found {} examples",
        examples.len()
    );
    let mut failures: Vec<String> = unattributed
        .into_iter()
        .map(|at| format!("{at}: a JSON request whose heading names no single tool"))
        .collect();
    for Example { at, tool, input } in examples {
        // A template slot (`octocode <tool> '…'`) is not a request.
        if tool.starts_with('<') {
            continue;
        }
        if ToolId::from_name(&tool).is_none() {
            failures.push(format!("{at}: unknown tool {tool}"));
            continue;
        }
        if let Err(error) = crate::contracts::validate(&tool, input.clone()) {
            let line = crate::contracts::format_input_error(&tool, &error)["error"].clone();
            failures.push(format!("{at}: native rejects {tool} {input}: {line}"));
        }
        let schema = published_input(&tool);
        let mut value = input.clone();
        if let Err(error) = validate_input_schema(schema, schema, &mut value, &mut Vec::new()) {
            let issue = error.issues.first().map(|issue| issue.message.clone());
            failures.push(format!(
                "{at}: the published schema rejects {tool} {input}: {issue:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Core's authored examples (one per first-call form, shown by `schema
/// --view variants` and `<tool> --help`) pass native admission too.
#[test]
fn every_core_example_is_accepted_by_native_admission() {
    let presentation = crate::contracts::cli_presentation().expect("presentation");
    let mut failures = Vec::new();
    let mut count = 0;
    for entry in presentation["tools"].as_array().expect("tools") {
        let tool = entry["name"].as_str().expect("name");
        for variant in entry["views"]["variants"]["variants"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(example) = variant.get("example") else {
                failures.push(format!("{tool} {}: no example", variant["name"]));
                continue;
            };
            count += 1;
            let input = serde_json::json!({ "queries": [example] });
            if let Err(error) = crate::contracts::validate(tool, input) {
                let line = crate::contracts::format_input_error(tool, &error)["error"].clone();
                failures.push(format!("{tool} {}: {line}", variant["name"]));
            }
        }
    }
    assert!(count >= ToolId::ALL.len(), "{count} examples");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
