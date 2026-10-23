use super::words;
use serde_json::{Map, Value};

/// A call object: `{tool, query}`.
pub(super) fn is_call(value: &Value) -> bool {
    value.get("tool").and_then(Value::as_str).is_some() && value.get("query").is_some()
}

/// A hint string the runtime prints for a call, `[name: ]<tool> {json}`,
/// as `[name: ]octocode <tool> '<json>'`; `None` for any other text.
pub(super) fn call_string(text: &str) -> Option<String> {
    let brace = text.find(" {")?;
    let (head, json) = text.split_at(brace);
    let json = json.trim_start();
    serde_json::from_str::<Value>(json).ok()?.as_object()?;
    let tool_start = head.rfind(' ').map_or(0, |i| i + 1);
    let (prefix, tool) = head.split_at(tool_start);
    octocode_native::id::ToolId::from_name(tool)?;
    Some(format!("{prefix}{}", command(tool, json)))
}

/// `value` without its calls, or `None` when nothing else is left.
pub(super) fn without_calls(value: &Value) -> Option<Value> {
    match value {
        call if is_call(call) => None,
        Value::String(text) if call_string(text).is_some() => None,
        Value::Object(map) => {
            let kept: Map<String, Value> = map
                .iter()
                .filter_map(|(key, value)| {
                    if CONTENT_KEYS.contains(&key.as_str()) {
                        return Some((key.clone(), value.clone()));
                    }
                    without_calls(value).map(|value| (key.clone(), value))
                })
                .collect();
            (!kept.is_empty()).then_some(Value::Object(kept))
        }
        Value::Array(items) => {
            let kept: Vec<Value> = items.iter().filter_map(without_calls).collect();
            (!kept.is_empty()).then_some(Value::Array(kept))
        }
        other => Some(other.clone()),
    }
}

/// `key: value` notes for every field; calls are left to [`follow_ups`].
pub(super) fn field_notes(prefix: &str, fields: &Map<String, Value>) -> Vec<String> {
    let mut lines = Vec::new();
    for (key, value) in fields {
        let value = if CONTENT_KEYS.contains(&key.as_str()) {
            value.clone()
        } else if let Some(value) = without_calls(value) {
            value
        } else {
            continue;
        };
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
                lines.push(format!("{prefix}{key}: {}", words(&fields)));
            }
            other => lines.push(format!("{prefix}{key}: {other}")),
        }
    }
    lines
}

/// Keys whose values are data, never calls: content (source text,
/// matches, diffs) and `source`, the read a result echoes as what it judged
/// (its follow-up is the result's own `read`).
pub(super) const CONTENT_KEYS: [&str; 7] = [
    "files", "content", "matches", "value", "patch", "body", "source",
];

/// Every follow-up call in a structured result, as `label: octocode …`
/// lines: `next.*`, call hints, `responsePagination.next`, a repair. A
/// single row's calls drop the `results[0].data.` prefix.
pub(in super::super) fn follow_ups(structured: &Value) -> Vec<String> {
    fn walk(value: &Value, label: &str, out: &mut Vec<String>) {
        if is_call(value) {
            let tool = value["tool"].as_str().unwrap_or_default();
            let mut line = format!("{label}: {}", command(tool, &value["query"].to_string()));
            if let Some(reason) = value.get("reason").and_then(Value::as_str) {
                line.push_str(&format!("  # {reason}"));
            }
            out.push(line);
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if CONTENT_KEYS.contains(&key.as_str()) {
                        continue;
                    }
                    let label = if label.is_empty() {
                        key.clone()
                    } else {
                        format!("{label}.{key}")
                    };
                    walk(value, &label, out);
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    match item.as_str().and_then(call_string) {
                        Some(call) if label.ends_with("hints") => {
                            out.push(format!("{label}: {call}"))
                        }
                        _ => walk(item, &format!("{label}[{index}]"), out),
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(structured, "", &mut out);
    let single = structured["results"]
        .as_array()
        .is_some_and(|rows| rows.len() == 1);
    out.into_iter()
        .map(|line| match line.strip_prefix("results[0].data.") {
            Some(rest) if single => rest.to_owned(),
            _ => line,
        })
        .collect()
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
