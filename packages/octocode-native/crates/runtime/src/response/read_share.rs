//! Source line pages share one response window. Each source row is read with
//! an equal share of the window ([`row_window`]), and the reader cuts its line
//! page so the whole row fits that share ([`fitted_lines`]): this boundary
//! accounts for JSON escaping, UTF-16 and source gutters, so no row is read
//! twice. A byte page or a completed ranked row without a replay plan remains
//! indivisible; one source line may itself exceed the share.

use crate::id::ToolId;
use octocode_engine::json_size::string_body_units;

/// Room for the envelope around the rows: `base`, `shared`, and the
/// response-level warning that names the partial rows.
const ENVELOPE_CHARS: usize = 512;

/// The response chars each row of a `tool` call of `rows` rows may take
/// from a `page`-char response: an equal share, less the envelope when the
/// rows read sources (their readers fit the share exactly).
pub(crate) fn row_window(tool: ToolId, page: usize, rows: usize) -> usize {
    let page = if tool.reads_source() {
        page.saturating_sub(ENVELOPE_CHARS)
    } else {
        page
    };
    (page / rows.max(1)).max(1)
}

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

/// Serialized source row cost including the line gutters added at publication.
pub(crate) fn row_chars(row: &serde_json::Value) -> usize {
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
    octocode_engine::json_size::utf16_units(row).unwrap_or(0) + lines * gutter
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_row_receives_a_positive_share_and_numbering_counts_utf16() {
        for rows in [1, 3, 20] {
            for budget in [1, 500, 20000] {
                assert!(row_window(ToolId::LocalFetch, budget, rows) > 0);
            }
        }
        assert_eq!(
            text_chars("😀\n", 1),
            octocode_engine::json_size::string_body_units("😀\n") + 3
        );
    }
}
