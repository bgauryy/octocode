//! Source line pages share one response window. Each source row is read with
//! an equal share of the window ([`row_window`]), and the reader cuts its line
//! page so the whole row fits that share ([`fitted_lines`]): this boundary
//! accounts for JSON escaping, UTF-16 and source gutters, so no row is read
//! twice. A byte page or a completed ranked row without a replay plan remains
//! indivisible; one source line may itself exceed the share.

use serde_json::{Map, Value};

use crate::id::ToolId;
use crate::id::continuation_channels::PAGES_KEY;
use octocode_engine::json_size::string_body_units;

/// Room for the envelope around the rows: `base`, `shared`, and the
/// response-level warning that names the partial rows.
const ENVELOPE_CHARS: usize = 512;

/// Room for a shortened row's `next.continueWindow` beyond the query it copies:
/// tool name, offset, unit, and the row's remaining-lines warning.
const CONTINUATION_EXTRA_CHARS: usize = 192;

/// The response chars each row of a `tool` call of `rows` rows may take
/// from a `page`-char response: an equal share, less the envelope when the
/// rows read sources (their readers fit the share exactly).
pub(crate) fn row_window(tool: ToolId, page: usize, rows: usize) -> usize {
    let page = if tool.output().reads_source() {
        page.saturating_sub(ENVELOPE_CHARS)
    } else {
        page
    };
    (page / rows.max(1)).max(1)
}

/// Chars the `<line>\t` gutter adds to each content line once the response
/// numbers it: the line number's digits plus the JSON-escaped tab.
fn gutter_chars(file: &Map<String, Value>) -> usize {
    if file
        .get("contentView")
        .and_then(Value::as_str)
        .is_some_and(|view| view != "none")
        || file
            .get("pagination")
            .and_then(|pagination| pagination.get("unit"))
            .and_then(Value::as_str)
            == Some("bytes")
    {
        return 0;
    }
    let last = file.get("totalLines").and_then(Value::as_u64).unwrap_or(0);
    last.max(1).to_string().len() + 2
}

/// Chars one content line takes in the response.
fn line_chars(line: &str, gutter: usize) -> usize {
    string_body_units(line) + gutter
}

/// Response chars of numbered source `text` from a source of `total_lines`
/// lines: each line's JSON text and its gutter.
pub(crate) fn text_chars(text: &str, total_lines: usize) -> usize {
    let gutter = total_lines.max(1).to_string().len() + 2;
    text.split_inclusive('\n')
        .map(|line| line_chars(line, gutter))
        .sum()
}

/// The row's `content` and its gutter, when the row is a successful read
/// with text.
fn content(row: &Value) -> Option<(&str, usize)> {
    let file = row.as_object()?;
    if file.get("error").is_some_and(|error| !error.is_null()) {
        return None;
    }
    let text = file.get("content")?.as_str()?;
    (!text.is_empty()).then(|| (text, gutter_chars(file)))
}

/// Estimated response chars of a row: its JSON plus the line-number gutter.
pub(crate) fn row_chars(row: &Value) -> usize {
    let gutter = row
        .as_object()
        .and_then(|file| {
            file.get("content")?
                .as_str()
                .map(|text| (text, gutter_chars(file)))
        })
        .map_or(0, |(text, gutter)| {
            text.split_inclusive('\n').count() * gutter
        });
    octocode_engine::json_size::utf16_units(row).unwrap_or(0) + gutter
}

/// Line windows keep their coordinate; a full read can become a line
/// window only at its start. Byte windows are never reshaped here.
fn pages_by_lines(query: &Value) -> bool {
    match query.get("unit").and_then(Value::as_str) {
        Some("lines") => true,
        Some(_) => false,
        None => {
            query.get("fullContent") != Some(&Value::Bool(true))
                || query.get("offset").and_then(Value::as_u64).unwrap_or(0) == 0
        }
    }
}

/// Lines of `text` that fit `allowance` chars; at least one, so every row
/// shows content.
fn lines_within(text: &str, gutter: usize, allowance: usize) -> usize {
    let mut used = 0;
    let mut lines = 0;
    for line in text.split_inclusive('\n') {
        let chars = line_chars(line, gutter);
        if lines > 0 && used + chars > allowance {
            break;
        }
        used += chars;
        lines += 1;
    }
    lines
}

/// The query row of the row's own next page.
fn read_continuation(row: &Value) -> Option<&Value> {
    row.get(PAGES_KEY)?
        .get("continueWindow")
        .and_then(crate::response::result::continuation_row)
}

/// How many lines of a source `row` (a reader's row as published, read by
/// `query`) fit `share` response chars, when the row is over its share and
/// pages by lines: the reader cuts that line page instead. `None` when the
/// row fits, is a byte page, is a ranked row its continuation cannot replay,
/// or shows no more lines than it would keep.
pub(crate) fn fitted_lines(row: &Value, query: &Value, share: usize) -> Option<usize> {
    let size = row_chars(row);
    let byte_page = row.pointer("/pagination/unit").and_then(Value::as_str) == Some("bytes");
    let selected_without_plan = row.get("selection").is_some_and(Value::is_object)
        && query.get("selection").is_none()
        && read_continuation(row)
            .and_then(|query| query.get("selection"))
            .is_none();
    if size <= share || !pages_by_lines(query) || byte_page || selected_without_plan {
        return None;
    }
    let (text, gutter) = content(row)?;
    let text_chars: usize = text
        .split_inclusive('\n')
        .map(|line| line_chars(line, gutter))
        .sum();
    let around = size.saturating_sub(text_chars)
        + octocode_engine::json_size::utf16_units(query).unwrap_or(0)
        + CONTINUATION_EXTRA_CHARS;
    let keep = lines_within(text, gutter, share.saturating_sub(around));
    (keep < text.split_inclusive('\n').count()).then_some(keep)
}

#[cfg(test)]
mod tests;
