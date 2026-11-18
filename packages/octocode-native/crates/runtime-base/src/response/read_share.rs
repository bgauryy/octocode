//! Source line pages share one response window. Each source row is read with
//! an equal share of what the envelope leaves ([`row_window`]), and the reader
//! cuts its line page so the whole row, as published, fits that share: it
//! counts each line as it renders (`LineBudget::rendered`: JSON escaping,
//! UTF-16 and the gutter), measures the row's skeleton ([`published_chars`])
//! and cuts again when the row is wider, so no row is read twice. A byte
//! page, a one-line page or a completed ranked row without a replay plan
//! remains indivisible; one source line may itself exceed the share.

use crate::id::ToolId;
use octocode_engine::json_size::{string_body_units, utf16_units};
use serde_json::{Value, json};

/// The response chars each row of a `tool` call of `rows` rows may take
/// from a `page`-char response: an equal share of what the envelope leaves
/// when the rows read sources (their readers fit the share exactly).
pub fn row_window(tool: ToolId, page: usize, rows: usize) -> usize {
    let page = if tool.reads_source() {
        page.saturating_sub(frame_chars(tool, rows))
    } else {
        page
    };
    (page / rows.max(1)).max(1)
}

/// Response chars around the `data` of `rows` result rows: the envelope and
/// each row's own wrapper (`{"index":…,"data":…}` and the comma between
/// rows), measured on that envelope with empty data.
fn frame_chars(tool: ToolId, rows: usize) -> usize {
    let rows = rows.max(1);
    let frame = super::rows::envelope(
        tool,
        (0..rows)
            .map(|index| super::rows::result_row(index, json!({}), None))
            .collect(),
    );
    // Each empty `data` is `{}`, which the row's own data replaces.
    utf16_units(&frame).unwrap_or(0).saturating_sub(2 * rows)
}

/// What a source read's row takes in a response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Published {
    /// Response chars of the row's `data`.
    pub row: usize,
    /// Of those, the chars of its `content` text (inside the quotes).
    pub text: usize,
}

/// Response chars of a source read's native `row` once published: its notices placed, its lines numbered, its complete blocks dropped, and each
/// call it continues by naming the cursor publication issues for it (the
/// same length as every issued cursor). The skeleton is `row - text`.
pub fn published_chars(row: &Value) -> Published {
    let mut staged = json!({"results":[{"index":0,"data":row}]});
    super::notices::place(&mut staged, "results");
    let data = &mut staged["results"][0]["data"];
    super::numbered::number_read_row(data);
    super::pages::publish_rows(&mut staged);
    let data = &mut staged["results"][0]["data"];
    as_cursors(data);
    Published {
        row: utf16_units(data).unwrap_or(0),
        text: data["content"].as_str().map_or(0, string_body_units),
    }
}

/// Each executable call under `value` with the input publication gives it:
/// its cursor ([`super::continuations`]); a related call keeps its input.
fn as_cursors(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let call = object.contains_key("tool")
                && object.contains_key("input")
                && object
                    .get("kind")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kind != "related");
            if call {
                object.insert(
                    "input".into(),
                    json!({"cursor": super::cursor::issued::placeholder()}),
                );
                return;
            }
            object.values_mut().for_each(as_cursors);
        }
        Value::Array(items) => items.iter_mut().for_each(as_cursors),
        _ => {}
    }
}

fn line_chars(line: &str, gutter: usize) -> usize {
    string_body_units(line) + gutter
}

/// Response chars of numbered source `text` from a source of `total_lines`
/// lines: each line's JSON text and its gutter.
pub fn text_chars(text: &str, total_lines: usize) -> usize {
    let gutter = total_lines.max(1).to_string().len() + 2;
    text.split_inclusive('\n')
        .map(|line| line_chars(line, gutter))
        .sum()
}

/// Serialized source row cost including the line gutters added at publication.
pub fn row_chars(row: &serde_json::Value) -> usize {
    let numbered = row
        .get("contentView")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|view| view != "none")
        || row
            .pointer("/pagination/unit")
            .and_then(serde_json::Value::as_str)
            == Some("bytes");
    let gutter = if numbered {
        0
    } else {
        row["totalLines"]
            .as_u64()
            .unwrap_or(0)
            .max(1)
            .to_string()
            .len()
            + 2
    };
    let lines = row["content"]
        .as_str()
        .map_or(0, |text| text.split_inclusive('\n').count());
    utf16_units(row).unwrap_or(0) + lines * gutter
}

#[cfg(test)]
mod tests;
