//! Text for every row without a line format. A row's `data` prints as
//! indented `key: value` text: a multi-line string as a `key: |` block of its
//! lines, an object or a list of objects nested under its key (`- ` leads
//! each list item), a list of strings one `- ` item per line, numbers and
//! flags inline. A list whose items all hold the same single field names it
//! once (`nodes[].node:`). Every field prints; nothing is clipped.
use super::follow::split_stated;
use super::{Row, tail, words};
use serde_json::{Map, Value};

/// A row as `key: value` text. The fields of `data` the trailer states
/// (done facts, warnings, hints) print there, as they do for a line format.
pub(super) fn generic(row: &Map<String, Value>, query: &Value) -> Row {
    let data = row
        .get("data")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let (stated, shown) = split_stated(data);
    let mut body = Vec::new();
    object(&shown, "", &mut body);
    tail(query, row, stated, Vec::new(), body)
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
    if let Value::Object(fields) = value
        && let Some(named) = floor_named(fields)
    {
        return entry(head, &Value::Object(named), indent, out);
    }
    // Items that all hold the same single field name it once, in the head:
    // `nodes[].node:`, then each item's value.
    if let Value::Array(items) = value
        && let Some(key) = head.strip_suffix(':')
        && let Some((field, values)) = only_field(items)
    {
        return entry(
            &format!("{key}[].{field}:"),
            &Value::Array(values),
            indent,
            out,
        );
    }
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
            // One layout per array: a row of single words stays inline only
            // when every row of its array does.
            let block = items.iter().any(needs_block);
            for item in items {
                list_item(item, &inner, block, out);
            }
        }
        other => out.push(format!("{head} {other}")),
    }
}

/// The field and the values of a list whose items are all objects holding
/// that one field; `None` for any other list.
fn only_field(items: &[Value]) -> Option<(String, Vec<Value>)> {
    let field = items.first()?.as_object()?.keys().next()?.clone();
    let values = items
        .iter()
        .map(|item| {
            let fields = item.as_object().filter(|fields| fields.len() == 1)?;
            // A nested single-field list would name two fields in one head.
            fields
                .get(&field)
                .filter(|value| !value.is_array())
                .cloned()
        })
        .collect::<Option<Vec<_>>>()?;
    Some((field, values))
}

/// `fields` with a count that is only a floor named as one: where
/// `countScope` is `loaded`, `totalItems` is the rows read so far, so it
/// prints as `totalItemsAtLeast`. `None` when `fields` holds no such floor.
pub(super) fn floor_named(fields: &Map<String, Value>) -> Option<Map<String, Value>> {
    if fields.get("countScope").and_then(Value::as_str) != Some("loaded")
        || !fields.contains_key("totalItems")
    {
        return None;
    }
    Some(
        fields
            .iter()
            .map(|(key, value)| {
                let key = if key == "totalItems" {
                    "totalItemsAtLeast"
                } else {
                    key
                };
                (key.to_owned(), value.clone())
            })
            .collect(),
    )
}

/// An object row that cannot read as one line of words.
fn needs_block(item: &Value) -> bool {
    matches!(item, Value::Object(fields) if !fields.is_empty() && !fields.values().all(is_word))
}

/// One list item: `- value`, or an object's first field on the `- ` line
/// and the rest aligned under it (`block`: for every object row, however
/// few words it has).
fn list_item(item: &Value, indent: &str, block: bool, out: &mut Vec<String>) {
    match item {
        Value::Object(fields) if !fields.is_empty() && (block || needs_block(item)) => {
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
