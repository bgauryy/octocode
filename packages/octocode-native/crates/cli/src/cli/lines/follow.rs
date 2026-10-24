use super::words;
use serde_json::{Map, Value};

/// Render each remaining field after declared calls have been separated.
pub(super) fn field_notes(prefix: &str, fields: &Map<String, Value>) -> Vec<String> {
    let mut lines = Vec::new();
    for (key, value) in fields {
        match value {
            Value::Array(items) if matches!(key.as_str(), "hints" | "warnings") => {
                for item in items {
                    let text = item
                        .as_str()
                        .map_or_else(|| item.to_string(), str::to_owned);
                    lines.push(format!("{prefix}{key}: {text}"));
                }
            }
            Value::String(text) => lines.push(format!("{prefix}{key}: {text}")),
            // A flat object reads as words: `summary: files 4 dirs 0`.
            Value::Object(fields) if fields.values().all(|value| !value.is_object()) => {
                lines.push(format!("{prefix}{key}: {}", words(fields)));
            }
            other => lines.push(format!("{prefix}{key}: {other}")),
        }
    }
    lines
}

/// Extract commands only from positions declared as calls by the output schema.
pub(super) fn separate_calls(
    tool: octocode_native::id::ToolId,
    structured: &Value,
) -> Option<(Value, Vec<String>)> {
    let mut calls = Vec::new();
    octocode_native::response::continuations::visit_calls(tool, structured, &mut |call, path| {
        calls.push((path.to_vec(), call.clone()));
    })
    .ok()?;
    let mut display = structured.clone();
    for (path, _) in calls.iter().rev() {
        remove_at(&mut display, path);
    }
    let single = structured["results"]
        .as_array()
        .is_some_and(|rows| rows.len() == 1);
    let notes = calls
        .into_iter()
        .map(|(path, call)| {
            let mut label = String::new();
            for part in path {
                if part.parse::<usize>().is_ok() {
                    label.push_str(&format!("[{part}]"));
                } else {
                    if !label.is_empty() {
                        label.push('.');
                    }
                    label.push_str(&part);
                }
            }
            if single {
                label = label
                    .strip_prefix("results[0].data.")
                    .unwrap_or(&label)
                    .to_owned();
            }
            let mut note = format!(
                "{label}: {}",
                command(
                    call["tool"].as_str().unwrap_or_default(),
                    &call["input"].to_string()
                )
            );
            if let Some(reason) = call.get("reason").and_then(Value::as_str) {
                note.push_str(&format!("  # {reason}"));
            }
            note
        })
        .collect();
    Some((display, notes))
}

fn remove_at(value: &mut Value, path: &[String]) -> bool {
    let Some((head, rest)) = path.split_first() else {
        return true;
    };
    match value {
        Value::Object(map) => {
            if map
                .get_mut(head)
                .is_some_and(|child| remove_at(child, rest))
            {
                map.shift_remove(head);
            }
            map.is_empty()
        }
        Value::Array(items) => {
            if let Ok(index) = head.parse::<usize>()
                && items
                    .get_mut(index)
                    .is_some_and(|child| remove_at(child, rest))
            {
                items.remove(index);
            }
            items.is_empty()
        }
        _ => false,
    }
}

/// `octocode <tool> '<json>'`, single-quoted for the shell that runs it:
/// POSIX, or PowerShell ([`powershell`]).
pub(in super::super) fn command(tool: &str, json: &str) -> String {
    if powershell() {
        // PowerShell doubles a quote inside single quotes; the JSON's own
        // double quotes reach the program intact from PowerShell 7.3.
        format!("octocode {tool} '{}'", json.replace('\'', "''"))
    } else {
        format!("octocode {tool} {}", octocode_io::shell::quote(json))
    }
}

/// Whether printed commands are for PowerShell: on Windows outside a POSIX
/// shell (Git Bash, MSYS and Cygwin set `MSYSTEM` or `SHELL`).
pub(super) fn powershell() -> bool {
    cfg!(windows) && std::env::var_os("MSYSTEM").is_none() && std::env::var_os("SHELL").is_none()
}

/// Whether every row of a result failed: its text then belongs on stderr.
pub(in super::super) fn all_rows_failed(structured: &Value) -> bool {
    structured["results"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["status"] == "error"))
}
