use super::outline::floor_named;
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
                let named = floor_named(fields);
                lines.push(format!(
                    "{prefix}{key}: {}",
                    words(named.as_ref().unwrap_or(fields))
                ));
            }
            other => lines.push(format!("{prefix}{key}: {other}")),
        }
    }
    lines
}

/// Fields that say whether an answer is whole, beside any object that holds
/// `hasMore`: they print in the stdout trailer, never only on stderr.
const INCOMPLETE: [&str; 5] = [
    "rowPart",
    "isPartial",
    "partialReasons",
    "totalItemsCapped",
    "unknownItems",
];

/// Whether a field decides "am I done": a page state (`hasMore`) or a mark
/// of an incomplete answer.
pub(super) fn decides_done(key: &str, value: &Value) -> bool {
    INCOMPLETE.contains(&key)
        || value
            .as_object()
            .is_some_and(|fields| fields.contains_key("hasMore"))
}

/// `fields` split into those that decide "am I done" and the rest.
pub(super) fn split_done(fields: Map<String, Value>) -> (Map<String, Value>, Map<String, Value>) {
    fields
        .into_iter()
        .partition(|(key, value)| decides_done(key, value))
}

/// Fields that say how to read an answer (what was redacted, why it is
/// partial, what to ask instead): like the done facts they print in the
/// stdout trailer, for every tool and row shape.
const ADVICE: [&str; 2] = ["warnings", "hints"];

/// `fields` split into those the stdout trailer states (the done facts and
/// the [`ADVICE`]) and the rest.
pub(super) fn split_stated(fields: Map<String, Value>) -> (Map<String, Value>, Map<String, Value>) {
    fields
        .into_iter()
        .partition(|(key, value)| decides_done(key, value) || ADVICE.contains(&key.as_str()))
}

/// The trailer lines of `stated` fields ([`split_stated`]): the done facts,
/// then the advice.
pub(super) fn stated_notes(stated: Map<String, Value>) -> (Vec<String>, Vec<String>) {
    let (done, advice) = split_done(stated);
    (field_notes("", &done), field_notes("", &advice))
}

/// A follow-up call the output schema declares, as a runnable command.
pub(super) struct Call {
    /// The row it belongs to; `None` for the envelope's.
    pub row: Option<usize>,
    /// Its place in the row's `data` (`next.nextPage`, `related[0]`) or in
    /// the envelope (`responsePagination.next`).
    pub label: String,
    pub command: String,
    /// A `related` call starts new work; every other continues, restarts or
    /// repairs this answer.
    pub lead: bool,
}

impl Call {
    fn note(&self) -> String {
        format!("{}: {}", self.label, self.command)
    }
}

/// The calls of `row` (`None`: the envelope's) as notes: the continuations
/// in run order (`next.nextPage` before any other), then the `related[]`
/// leads.
pub(super) fn notes_of(calls: &[Call], row: Option<usize>) -> (Vec<String>, Vec<String>) {
    let own = || calls.iter().filter(move |call| call.row == row);
    let mut continuations: Vec<&Call> = own().filter(|call| !call.lead).collect();
    continuations.sort_by_key(|call| call.label != "next.nextPage");
    (
        continuations.into_iter().map(Call::note).collect(),
        own().filter(|call| call.lead).map(Call::note).collect(),
    )
}

/// Extract commands only from positions declared as calls by the output schema.
pub(super) fn separate_calls(
    tool: octocode_native::id::ToolId,
    structured: &Value,
) -> Option<(Value, Vec<Call>)> {
    let mut calls = Vec::new();
    octocode_native::response::continuations::visit_calls(tool, structured, &mut |call, path| {
        calls.push((path.to_vec(), call.clone()));
    })
    .ok()?;
    let mut display = structured.clone();
    for (path, _) in calls.iter().rev() {
        remove_at(&mut display, path);
    }
    let notes = calls
        .into_iter()
        .map(|(path, call)| {
            // `results[N].data.` names the row, which the heading already does.
            let (row, rest) = match path.as_slice() {
                [results, index, rest @ ..] if results == "results" => {
                    let rest = match rest {
                        [data, rest @ ..] if data == "data" => rest,
                        rest => rest,
                    };
                    (index.parse::<usize>().ok(), rest)
                }
                rest => (None, rest),
            };
            let mut label = String::new();
            for part in rest {
                if part.parse::<usize>().is_ok() {
                    label.push_str(&format!("[{part}]"));
                } else {
                    if !label.is_empty() {
                        label.push('.');
                    }
                    label.push_str(part);
                }
            }
            let mut command = command(
                call["tool"].as_str().unwrap_or_default(),
                &call["input"].to_string(),
            );
            if let Some(reason) = call.get("reason").and_then(Value::as_str) {
                command.push_str(&format!("  # {reason}"));
            }
            Call {
                row,
                label,
                command,
                lead: call["kind"] == "related",
            }
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
