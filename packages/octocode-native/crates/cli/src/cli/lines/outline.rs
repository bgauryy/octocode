//! Text for every row without a line format. A row's `data` prints as
//! indented `key: value` text: a multi-line string as a `key: |` block of its
//! lines, an object or a list of objects nested under its key (`- ` leads
//! each list item), a list of strings one `- ` item per line, numbers and
//! flags inline. A list whose items all hold the same single field names it
//! once (`nodes[].node:`); a list of flat rows prints as a [`table`]. A
//! field every object of a list holds with one single-word value is stated
//! once after the list's key in `[ … ]` ([`once::item_constants`]). A
//! top-level block of numbered source lines (`N<TAB>text`, a read's
//! `content`) prints them unindented: their numbers set them apart. Every
//! field prints; nothing is clipped.
use super::follow::split_stated;
use super::once;
use super::table::{is_word, table};
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

/// `head` (`key:` or `-`) and `value`, nested `indent` deep.
fn entry(head: &str, value: &Value, indent: &str, out: &mut Vec<String>) {
    let inner = format!("{indent}  ");
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
    if let Value::Array(items) = value
        && let Some(key) = head.strip_suffix(':')
        && let Some(mut lines) = table(key.trim_start(), items, indent)
    {
        return out.append(&mut lines);
    }
    match value {
        Value::String(text) if text.contains('\n') => {
            out.push(format!("{head} |"));
            match numbered_lines(text).filter(|_| indent.is_empty()) {
                Some(lines) => out.extend(lines.into_iter().map(str::to_owned)),
                None => out.extend(body_lines(text).map(|line| format!("{inner}{line}"))),
            }
        }
        Value::String(text) if text.is_empty() => out.push(format!("{head} \"\"")),
        Value::String(text) => out.push(format!("{head} {text}")),
        Value::Object(fields) if fields.is_empty() => out.push(format!("{head} {{}}")),
        // A flat object of single words reads as words: `unit lines offset 0 hasMore`.
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
            let constants = once::item_constants(items, true);
            if constants.is_empty() {
                out.push(head.to_owned());
            } else {
                out.push(format!("{head}  [{}]", words(&constants)));
            }
            let items: Vec<Value> = items
                .iter()
                .map(|item| once::without_constants(item, &constants))
                .collect();
            // One layout per array: a row of single words stays inline only
            // when every row of its array does.
            let block = items.iter().any(needs_block);
            for item in &items {
                list_item(item, &inner, block, out);
            }
        }
        other => out.push(format!("{head} {other}")),
    }
}

/// The lines of a multi-line value as the layout prints them: a final line
/// break ends the last line, so it starts no line of its own.
fn body_lines(text: &str) -> std::str::Split<'_, char> {
    text.strip_suffix('\n').unwrap_or(text).split('\n')
}

/// The lines of a value that is numbered source lines (`N<TAB>text`, the
/// value's final line break ending its last line); `None` for any other.
fn numbered_lines(text: &str) -> Option<Vec<&str>> {
    let lines: Vec<&str> = body_lines(text).collect();
    lines
        .iter()
        .all(|line| {
            line.split_once('\t').is_some_and(|(number, _)| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
        .then_some(lines)
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
