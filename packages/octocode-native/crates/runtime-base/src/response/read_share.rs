//! Source line pages share one response window. Each source row is first
//! read with an equal share of what the envelope leaves ([`row_window`]), and
//! the reader cuts its line page so the whole row, as published, fits that
//! share: it counts each line as it renders (`LineBudget::rendered`: JSON
//! escaping, UTF-16 and the gutter), measures the row's skeleton
//! ([`published_chars`]) and cuts again when the row is wider. A byte page, a
//! one-line page or a completed ranked row without a replay plan remains
//! indivisible; one source line may itself exceed the share.
//!
//! Rows that [`fill_together`] then fill the page like water: a row that
//! finishes under its share leaves the rest to the rows still reading, which
//! are cut again from their kept views with the wider share ([`widened`]),
//! until no row still reading would gain. The rows still reading continue in
//! one call, which fills its page the same way, so a batch never takes more
//! calls than the rows' pages packed one after another.

use crate::id::{ToolFamily, ToolId};
use octocode_engine::json_size::{string_body_units, utf16_units};
use serde_json::{Value, json};

/// The response chars each row of a `tool` call of `rows` rows may take
/// from a `page`-char response: an equal share of what the envelope leaves
/// when the rows read sources (their readers fit the share exactly).
pub fn row_window(tool: ToolId, page: usize, rows: usize) -> usize {
    (rows_room(tool, page, rows) / rows.max(1)).max(1)
}

/// The response chars the `rows` rows of a `tool` call share of a
/// `page`-char response: what the envelope leaves when the rows read sources.
pub fn rows_room(tool: ToolId, page: usize, rows: usize) -> usize {
    if tool.reads_source() {
        page.saturating_sub(frame_chars(tool, rows))
    } else {
        page
    }
}

/// Whether the rows of one `tool` call fill the response page together
/// ([`widened`]) and continue together: rows that read sources, read again
/// with a wider share at no provider request ([`reread_is_free`]). A row a
/// provider judges is never read again ([`reads_again`]).
#[must_use]
pub fn fill_together(tool: ToolId) -> bool {
    tool.reads_source() && reread_is_free(tool)
}

/// Whether reading a `tool` row again costs no provider request: a local
/// read reads this host's file again; a GitHub file read reads its body at
/// the commit SHA its ref resolved to, an answer pinned to that commit and
/// kept, with the ref from the batch's resolutions, so a second read sends
/// nothing.
#[must_use]
pub fn reread_is_free(tool: ToolId) -> bool {
    match tool.family() {
        ToolFamily::Local => true,
        ToolFamily::GitHub => tool == ToolId::GhGetFileContent,
        ToolFamily::Remote => false,
    }
}

/// Whether the row of `query` may be read again with a wider share: a row
/// with a provider field (`classification`, `selection`) is judged, and a
/// second read would ask the judge again.
#[must_use]
pub fn reads_again(query: &Value) -> bool {
    crate::contracts::PROVIDER_FIELDS
        .iter()
        .all(|field| query.get(field).is_none())
}

/// Whether a source row's page ends before its read does: it continues its
/// own walk (`next.nextPage`).
#[must_use]
pub fn reads_on(row: &Value) -> bool {
    row.pointer("/next/nextPage").is_some()
}

/// One row of a batch that fills its page together, as last read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filled {
    /// Response chars of the row's `data` ([`published_chars`]).
    pub chars: usize,
    /// The row reads on ([`reads_on`]) and may be read again ([`reads_again`]).
    pub open: bool,
}

/// The share the open rows of `rows` read with next: what the settled rows
/// leave of the `room` the batch shares, split evenly among the open rows.
/// `None` when no row is open or none would read more than the `share` it
/// read with: water fills the page until the rows still reading gain nothing.
#[must_use]
pub fn widened(room: usize, share: usize, rows: &[Filled]) -> Option<usize> {
    let open = rows.iter().filter(|row| row.open).count();
    if open == 0 {
        return None;
    }
    let settled = rows
        .iter()
        .filter(|row| !row.open)
        .map(|row| row.chars)
        .fold(0usize, usize::saturating_add);
    let wider = room.saturating_sub(settled) / open;
    (wider > share).then_some(wider)
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
