//! A list of flat rows as a table: `key[]: col col text` names the columns
//! once, then each row is one indented line of its values in that order.
//! A row qualifies when every field is a scalar on one line and at most one
//! field of the list holds free text (a value with a space, or empty); that
//! field is the last column, so it reads to the end of the line. Columns
//! keep the rows' field order otherwise, which puts identity first. A field
//! with the same single-word value on every row is stated once after the
//! head in `[ … ]`, as a path's other fields are. A row without a field
//! prints `-` in its column. Any other list keeps its `key: value` layout.
use super::outline::floor_named;
use super::words;
use serde_json::{Map, Value};

/// The mark of a field a row does not have.
const ABSENT: &str = "-";

/// A value no wider than one word: it reads unambiguously among words.
pub(super) fn is_word(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.is_empty() && !text.contains(char::is_whitespace),
        Value::Number(_) | Value::Bool(_) | Value::Null => true,
        _ => false,
    }
}

/// `items` as table lines under `key` at `indent`; `None` when they are not
/// flat rows with at most one free-text field.
pub(super) fn table(key: &str, items: &[Value], indent: &str) -> Option<Vec<String>> {
    if items.is_empty() {
        return None;
    }
    let rows: Vec<Map<String, Value>> = items
        .iter()
        .map(|item| {
            let fields = item.as_object().filter(|fields| !fields.is_empty())?;
            Some(floor_named(fields).unwrap_or_else(|| fields.clone()))
        })
        .collect::<Option<_>>()?;
    let mut columns: Vec<&str> = Vec::new();
    let mut free = None;
    for row in &rows {
        for (name, value) in row.iter() {
            match value {
                Value::String(text) if text.contains('\n') || text == ABSENT => return None,
                Value::Array(_) | Value::Object(_) => return None,
                _ => {}
            }
            if !columns.contains(&name.as_str()) {
                columns.push(name);
            }
            if !is_word(value) {
                match free {
                    Some(other) if other != name.as_str() => return None,
                    _ => free = Some(name.as_str()),
                }
            }
        }
    }
    if let Some(free) = free {
        columns.retain(|name| *name != free);
        columns.push(free);
    }
    // Only a list states a field once for all its rows; the first column
    // (identity) stays on each row.
    let mut stated = Map::new();
    if rows.len() > 1 {
        for name in columns.iter().skip(1) {
            let first = rows[0].get(*name).filter(|value| is_word(value));
            if let Some(first) = first
                && rows.iter().all(|row| row.get(*name) == Some(first))
            {
                stated.insert((*name).to_owned(), first.clone());
            }
        }
        columns.retain(|name| !stated.contains_key(*name));
    }
    let mut head = format!("{indent}{key}[]: {}", columns.join(" "));
    if !stated.is_empty() {
        head.push_str(&format!("  [{}]", words(&stated)));
    }
    let mut lines = vec![head];
    for row in &rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|name| match row.get(*name) {
                None => ABSENT.to_owned(),
                Some(Value::String(text)) if text.is_empty() => "\"\"".to_owned(),
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
            })
            .collect();
        lines.push(format!("{indent}  {}", cells.join(" ")));
    }
    Some(lines)
}
