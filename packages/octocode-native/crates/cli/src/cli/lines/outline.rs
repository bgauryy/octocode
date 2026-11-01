//! Text for every result without a line format, and for batches. A row's
//! `data` prints as indented `key: value` text: a multi-line string as a
//! `key: |` block of its lines, an object or a list of objects nested under
//! its key (`- ` leads each list item), a list of strings one `- ` item per
//! line, numbers and flags inline. Every field prints; nothing is clipped.
//! A batch prints each row under `## results[N]`, in its line format when
//! the tool has one, and prefixes the row's stderr notes the same way.
use super::{Lines, envelope_notes, notes, render_data, words};
use octocode_native::id::ToolId;
use serde_json::{Map, Value, json};

/// A single-row result as `key: value` text; its status, row and envelope
/// fields and the root are stderr notes, as for a line format.
pub(super) fn generic(structured: &Value, query: &Value, cwd: Option<&std::path::Path>) -> Lines {
    let empty = Map::new();
    let row = structured["results"][0].as_object().unwrap_or(&empty);
    let mut out = Vec::new();
    if let Some(data) = row.get("data").and_then(Value::as_object) {
        object(data, "", &mut out);
    }
    Lines {
        out: out.join("\n"),
        notes: notes(structured, query, row, Map::new(), Vec::new(), cwd),
    }
}

/// Each row of a batch under `## results[N]` (its `index` beside it when a
/// continuation renumbered the rows), then the envelope's notes once.
pub(super) fn batch(
    tool: ToolId,
    structured: &Value,
    query: &Value,
    cwd: Option<&std::path::Path>,
) -> Lines {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let rows = structured["results"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for (position, row) in rows.iter().enumerate() {
        let label = format!("results[{position}]");
        let index = row["index"].as_u64();
        out.push(match index {
            Some(index) if index != position as u64 => format!("## {label} index {index}"),
            _ => format!("## {label}"),
        });
        // The row's own query: the line format reads it (context lines,
        // operation). A continuation names none, so it passes the cursor.
        let asked = index
            .and_then(|index| query["queries"].get(usize::try_from(index).ok()?))
            .map_or_else(|| query.clone(), |asked| json!({"queries": [asked]}));
        let single = json!({"results": [row]});
        let lines = render_data(tool, &single, &asked, cwd)
            .unwrap_or_else(|| generic(&single, &asked, cwd));
        if !lines.out.is_empty() {
            out.push(lines.out);
        }
        notes.extend(
            lines
                .notes
                .lines()
                .map(|note| format!("# {label} {}", note.strip_prefix("# ").unwrap_or(note))),
        );
    }
    notes.extend(
        envelope_notes(structured, cwd)
            .iter()
            .map(|note| format!("# {note}")),
    );
    Lines {
        out: out.join("\n"),
        notes: notes.join("\n"),
    }
}

/// Each field of `fields` at `indent`.
pub(super) fn object(fields: &Map<String, Value>, indent: &str, out: &mut Vec<String>) {
    for (key, value) in fields {
        entry(&format!("{indent}{key}:"), value, indent, out);
    }
}

/// A value no wider than one word: it reads unambiguously among words.
fn is_word(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.is_empty() && !text.contains(char::is_whitespace),
        Value::Number(_) | Value::Bool(_) | Value::Null => true,
        _ => false,
    }
}

/// `head` (`key:` or `-`) and `value`, nested `indent` deep.
fn entry(head: &str, value: &Value, indent: &str, out: &mut Vec<String>) {
    let inner = format!("{indent}  ");
    match value {
        Value::String(text) if text.contains('\n') => {
            out.push(format!("{head} |"));
            out.extend(text.split('\n').map(|line| format!("{inner}{line}")));
        }
        Value::String(text) if text.is_empty() => out.push(format!("{head} \"\"")),
        Value::String(text) => out.push(format!("{head} {text}")),
        Value::Object(fields) if fields.is_empty() => out.push(format!("{head} {{}}")),
        // A flat object of single words reads as words: `page 1 hasMore`.
        Value::Object(fields) if fields.values().all(is_word) => {
            out.push(format!("{head} {}", words(fields)));
        }
        Value::Object(fields) => {
            out.push(head.to_owned());
            object(fields, &inner, out);
        }
        Value::Array(items) if items.is_empty() => out.push(format!("{head} []")),
        Value::Array(items) if items.iter().all(|item| is_word(item) && !item.is_string()) => {
            let items: Vec<String> = items.iter().map(Value::to_string).collect();
            out.push(format!("{head} {}", items.join(", ")));
        }
        Value::Array(items) => {
            out.push(head.to_owned());
            for item in items {
                list_item(item, &inner, out);
            }
        }
        other => out.push(format!("{head} {other}")),
    }
}

/// One list item: `- value`, or an object's first field on the `- ` line
/// and the rest aligned under it.
fn list_item(item: &Value, indent: &str, out: &mut Vec<String>) {
    match item {
        Value::Object(fields) if !fields.is_empty() && !fields.values().all(is_word) => {
            let start = out.len();
            let nested = format!("{indent}  ");
            object(fields, &nested, out);
            if let Some(first) = out.get_mut(start) {
                *first = format!("{indent}- {}", &first[nested.len()..]);
            }
        }
        _ => entry(&format!("{indent}-"), item, indent, out),
    }
}

#[cfg(test)]
mod tests;
