use super::table::list_cell;
use super::{outline, words};
use serde_json::{Map, Value};

/// Render each remaining field after declared calls have been separated, as
/// text: a flat object as words, a flat list as one cell, and any other
/// object or list (a list of `diagnostics`, nested counts) in the outline
/// and table layout a row's own fields print in, one line each.
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
            // A flat object reads as words: `summary: fileCount 4 dirCount 0`.
            Value::Object(fields) if fields.values().all(is_flat) => {
                lines.push(format!("{prefix}{key}: {}", words(fields)));
            }
            Value::Array(items) if is_flat(value) => match list_cell(items) {
                Some(Value::String(cell)) => lines.push(format!("{prefix}{key}: {cell}")),
                _ => nested(prefix, key, value, &mut lines),
            },
            Value::Array(_) | Value::Object(_) => nested(prefix, key, value, &mut lines),
            other => lines.push(format!("{prefix}{key}: {other}")),
        }
    }
    lines
}

/// Whether `value` reads as words inside one note: a scalar, or a list of
/// scalars.
fn is_flat(value: &Value) -> bool {
    match value {
        Value::Object(_) => false,
        Value::Array(items) => items
            .iter()
            .all(|item| !item.is_object() && !item.is_array()),
        _ => true,
    }
}

/// `key` and its nested `value` as the outline prints a row's fields: a
/// list of flat rows as a table, anything else indented under its key.
fn nested(prefix: &str, key: &str, value: &Value, lines: &mut Vec<String>) {
    let mut field = Map::new();
    field.insert(key.to_owned(), value.clone());
    let mut block = Vec::new();
    outline::object(&field, "", &mut block);
    lines.extend(block.into_iter().map(|line| format!("{prefix}{line}")));
}

/// Fields that say whether an answer is whole, beside any page state: they
/// print in the stdout trailer, never only on stderr.
const INCOMPLETE: [&str; 4] = ["rowPart", "isPartial", "partialReasons", "unknownItems"];

/// Whether a field decides "am I done": a page state ([`is_page`]: where a
/// window sits, or an exact count) or a mark of an incomplete answer. The
/// reply's `next` call, present while more remains, prints with the calls.
pub(super) fn decides_done(key: &str, value: &Value) -> bool {
    INCOMPLETE.contains(&key) || value.as_object().is_some_and(is_page)
}

/// Whether `fields` is a page state: it places a page (`offset`) and states
/// nothing but a page's own facts.
pub(super) fn is_page(fields: &Map<String, Value>) -> bool {
    fields.contains_key("offset")
        && fields.keys().all(|key| {
            matches!(
                key.as_str(),
                "unit" | "offset" | "length" | "total" | "oversized"
            )
        })
}

/// Whether a field prints in the trailer wherever it is stated: a done fact
/// ([`decides_done`]) or advice on reading the answer ([`ADVICE`]).
pub(super) fn in_trailer(key: &str, value: &Value) -> bool {
    decides_done(key, value) || ADVICE.contains(&key)
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

/// Fields that say which source an answer read: the ref it was asked at,
/// the commit it resolved to and where a materialized or cloned copy
/// lives. They print in the stdout trailer, as the identity of the content
/// beside them.
const IDENTITY: [&str; 3] = ["ref", "commitSha", "location"];

/// When a saved answer was observed: present only on an answer replayed
/// from the provider cache, it dates the whole answer, never one of its rows.
const OBSERVED_AT: &str = "observedAt";

/// `fields` split into those the stdout trailer states (the done facts, when
/// a replayed answer was observed, the [`IDENTITY`] and the [`ADVICE`]) and
/// the rest.
pub(super) fn split_stated(fields: Map<String, Value>) -> (Map<String, Value>, Map<String, Value>) {
    fields.into_iter().partition(|(key, value)| {
        decides_done(key, value)
            || key == OBSERVED_AT
            || ADVICE.contains(&key.as_str())
            || IDENTITY.contains(&key.as_str())
    })
}

/// The trailer lines of `stated` fields ([`split_stated`]): the done facts
/// and when a replayed answer was observed (`observedAt <time> (cached)`),
/// then the advice.
pub(super) fn stated_notes(mut stated: Map<String, Value>) -> (Vec<String>, Vec<String>) {
    let observed = stated.shift_remove(OBSERVED_AT);
    let (done, advice) = split_done(stated);
    let mut facts = field_notes("", &done);
    if let Some(observed) = observed {
        let at = observed
            .as_str()
            .map_or_else(|| observed.to_string(), str::to_owned);
        facts.push(format!("{OBSERVED_AT} {at} (cached)"));
    }
    (facts, field_notes("", &advice))
}

/// A follow-up call the output schema declares, as printed: a runnable
/// command, or the call itself.
pub(super) struct Call {
    /// The row it belongs to; `None` for the envelope's.
    pub row: Option<usize>,
    /// Its place in the row's `data` (`next.readDeclaration`, `related[0]`)
    /// or in the envelope (`next`).
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
/// in run order (the reply's `next` before any other), then the
/// `related[]` leads.
pub(super) fn notes_of(calls: &[Call], row: Option<usize>) -> (Vec<String>, Vec<String>) {
    let own = || calls.iter().filter(move |call| call.row == row);
    let mut continuations: Vec<&Call> = own().filter(|call| !call.lead).collect();
    continuations.sort_by_key(|call| call.label != "next");
    (
        continuations.into_iter().map(Call::note).collect(),
        own().filter(|call| call.lead).map(Call::note).collect(),
    )
}

/// Extract commands only from positions declared as calls by the output schema.
#[cfg(test)]
pub(super) fn separate_calls(
    tool: octocode_runtime_base::id::ToolId,
    structured: &Value,
) -> Option<(Value, Vec<Call>)> {
    separate_calls_as(tool, structured, false)
}

/// [`separate_calls`], each call printed as a shell command with its reason
/// after it, or (`as_json`) as the call object itself (`tool`, `input` and
/// any `reason`, compact), which a client sends unchanged.
pub(super) fn separate_calls_as(
    tool: octocode_runtime_base::id::ToolId,
    structured: &Value,
    as_json: bool,
) -> Option<(Value, Vec<Call>)> {
    let mut calls = Vec::new();
    octocode_runtime_base::response::continuations::visit_calls(
        tool,
        structured,
        &mut |call, path| {
            calls.push((path.to_vec(), call.clone()));
        },
    )
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
            let command = if as_json {
                call.to_string()
            } else {
                let mut command = command(
                    call["tool"].as_str().unwrap_or_default(),
                    &call["input"].to_string(),
                );
                if let Some(reason) = call.get("reason").and_then(Value::as_str) {
                    command.push_str(&format!("  # {reason}"));
                }
                command
            };
            Call {
                row,
                label,
                command,
                // A served call states no kind: the `related` list that holds
                // it says it starts new work.
                lead: rest
                    .iter()
                    .rev()
                    .find(|part| part.parse::<usize>().is_err())
                    .is_some_and(|key| key == "related"),
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
