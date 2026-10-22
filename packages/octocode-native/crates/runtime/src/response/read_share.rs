//! Source line pages share one response window. Each source row is read with
//! an equal share of the window ([`row_window`]), and the reader cuts its line
//! page so the whole row fits that share ([`fitted_lines`]): this boundary
//! accounts for JSON escaping, UTF-16 and source gutters, so no row is read
//! twice. A byte page or a completed ranked row without a replay plan remains
//! indivisible; one source line may itself exceed the share.

use serde_json::{Map, Value};

use crate::id::ToolId;
use crate::id::continuation_channels::PAGES_KEY;
use crate::response::stream_page::json_text_chars;

/// Room for the envelope around the rows: `base`, `shared`, and the
/// response-level warning that names the partial rows.
const ENVELOPE_CHARS: usize = 512;

/// Room for a shortened row's `next.continueWindow` beyond the query it copies:
/// tool name, offset, unit, and the row's remaining-lines warning.
const CONTINUATION_EXTRA_CHARS: usize = 192;

/// Tools whose rows each carry one source `content` string.
fn reads_source(tool: ToolId) -> bool {
    tool.output().text_shape() == crate::response::output::TextShape::FileText
}

/// The response chars each row of a `tool` call of `rows` rows may take
/// from a `page`-char response: an equal share, less the envelope when the
/// rows read sources (their readers fit the share exactly).
pub(crate) fn row_window(tool: ToolId, page: usize, rows: usize) -> usize {
    let page = if reads_source(tool) {
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
    json_text_chars(line) + gutter
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
mod tests {
    use super::*;
    use serde_json::json;

    fn local_row(lines: usize, width: usize) -> Value {
        let text: String = (1..=lines)
            .map(|line| format!("{line:05}{}\n", "x".repeat(width)))
            .collect();
        json!({"path":"a.txt","content":text,"totalLines":lines})
    }

    fn query(extra: Value) -> Value {
        let mut query = json!({"path":"a.txt"});
        for (key, value) in extra.as_object().into_iter().flatten() {
            query[key] = value.clone();
        }
        query
    }

    #[test]
    fn source_rows_share_the_page_less_the_envelope_and_others_split_it() {
        assert_eq!(
            row_window(ToolId::LocalFetch, 20_000, 3),
            (20_000 - ENVELOPE_CHARS) / 3
        );
        assert_eq!(
            row_window(ToolId::GhGetFileContent, 20_000, 1),
            20_000 - ENVELOPE_CHARS
        );
        assert_eq!(row_window(ToolId::LocalSearch, 20_000, 4), 5_000);
        assert_eq!(row_window(ToolId::LocalFetch, 100, 0), 1);
    }

    #[test]
    fn a_row_within_its_share_keeps_its_page() {
        assert_eq!(
            fitted_lines(&local_row(10, 20), &query(json!({})), 10_000),
            None
        );
        let explicit = local_row(7, 60);
        assert_eq!(
            fitted_lines(&explicit, &query(json!({"length":7})), 20_000),
            None
        );
    }

    #[test]
    fn a_row_over_its_share_keeps_the_lines_that_fit() {
        let share = row_window(ToolId::LocalFetch, 20_000, 3);
        let lines = fitted_lines(&local_row(300, 60), &query(json!({})), share).expect("cut");
        assert!((50..300).contains(&lines), "{lines}");
        let row = local_row(lines, 60);
        let continued = row_chars(&row)
            + octocode_engine::json_size::utf16_units(&query(json!({}))).unwrap_or(0)
            + CONTINUATION_EXTRA_CHARS;
        assert!(continued <= share, "{continued} > {share}");
    }

    #[test]
    fn byte_pages_are_not_reshaped() {
        for asked in [
            json!({"unit": "bytes", "length": 300}),
            json!({"fullContent": true, "offset": 4096}),
            json!({"unit": "bytes", "offset": 10}),
        ] {
            assert_eq!(
                fitted_lines(&local_row(300, 60), &query(asked.clone()), 6_000),
                None,
                "{asked}"
            );
        }
        let mut paged = local_row(300, 60);
        paged["pagination"] = json!({"unit": "bytes"});
        assert_eq!(fitted_lines(&paged, &query(json!({})), 6_000), None);
    }

    /// A ranked row its continuation cannot replay, and one line, are
    /// indivisible.
    #[test]
    fn indivisible_rows_keep_their_page() {
        let mut ranked = local_row(900, 60);
        ranked["selection"] =
            json!({"status":"complete","scope":"selectedView","selectedItems":2,"totalItems":2});
        assert_eq!(fitted_lines(&ranked, &query(json!({})), 20_000), None);
        ranked["next"] = json!({"continueWindow":{"tool":"localFetch","query":{"queries":[{
            "path":"a.txt", "offset":900, "selection":"native-plan"
        }]}}});
        assert!(fitted_lines(&ranked, &query(json!({})), 20_000).is_some());
        assert_eq!(
            fitted_lines(&local_row(1, 60_000), &query(json!({})), 20_000),
            None
        );
        let mut failed = local_row(900, 60);
        failed["error"] = json!("gone");
        assert_eq!(fitted_lines(&failed, &query(json!({})), 20_000), None);
    }

    fn read_local(
        query: &Value,
        policy: &crate::policy::path::PathPolicy,
        security: &crate::security::ContentSecurity,
        window: Option<usize>,
    ) -> Value {
        let prepared =
            crate::contracts::prepare_many_and_validate("localFetch", json!({"queries":[query]}))
                .expect("canonical query");
        let request = serde_json::from_value(prepared[0].clone()).expect("query");
        let result = crate::tools::local_fetch::execute_local_fetch(
            &request,
            policy,
            security,
            &crate::execution::cancel::NeverCancel,
            window,
            None,
        );
        assert_eq!(
            result.status,
            crate::tools::local_fetch::ReadStatus::Success,
            "{result:?}"
        );
        serde_json::to_value(result).expect("result")
    }

    /// The lines of every page of a walk from `query`, each read with its
    /// share of `window` (`None`: the reader's own page), and the pages.
    fn walk_local(
        mut query: Value,
        policy: &crate::policy::path::PathPolicy,
        window: Option<usize>,
    ) -> (Vec<String>, usize) {
        let security = crate::security::ContentSecurity::new();
        let share = window.map(|window| row_window(ToolId::LocalFetch, window, 1));
        let mut records = Vec::new();
        for pages in 1..=200 {
            let row = read_local(&query, policy, &security, share);
            let mut wire = crate::tools::test_support::published(ToolId::LocalFetch, &query, &row);
            crate::response::numbered::number_read_rows(ToolId::LocalFetch, &mut wire);
            crate::tools::test_support::checks::assert_output_contract("localFetch", &wire);
            if let Some(window) = window {
                let size = octocode_engine::json_size::utf16_units(&wire).unwrap_or(0);
                assert!(size <= window, "{size} > {window}");
            }
            let data = &wire["results"][0]["data"];
            records.extend(
                data["content"]
                    .as_str()
                    .expect("content")
                    .lines()
                    .filter(|line| *line != "--")
                    .map(str::to_owned),
            );
            let Some(next) = data["next"].get("continueWindow") else {
                return (records, pages);
            };
            let prepared =
                crate::contracts::prepare_many_and_validate("localFetch", next["query"].clone())
                    .expect("canonical continuation");
            query = prepared[0].clone();
        }
        panic!("read failed to terminate");
    }

    #[test]
    fn single_short_line_reads_fit_the_numbered_json_window_and_copy_every_page() {
        let root = crate::tools::test_support::Fixture::new();
        let path = root.write("short.txt", "x\n".repeat(12_000));
        let policy = crate::tools::test_support::workspace_policy(&root.0);
        for window in [20_000, 50_000] {
            for extra in [
                json!({"fullContent":true}),
                json!({"unit":"lines","length":50_000}),
                json!({"ranges":["1-3000","9001-12000"]}),
            ] {
                let mut query = json!({"path":path});
                query
                    .as_object_mut()
                    .expect("object")
                    .extend(extra.as_object().expect("extra").clone());
                let (expected, _) = walk_local(query.clone(), &policy, None);
                let (actual, _) = walk_local(query, &policy, Some(window));
                assert_eq!(actual, expected);
                assert_eq!(
                    actual
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len(),
                    actual.len()
                );
            }
        }
    }

    /// Every view of a late-match source pages identically walked at the
    /// reader's own page size and walked through shares of each of
    /// `windows`. Its escape-heavy lines put a byte-sized page over its
    /// share, so every share is met by the reader's fit.
    fn late_and_transformed_pages_keep_source_coordinates(windows: &[usize]) {
        let root = crate::tools::test_support::Fixture::new();
        let literal = format!("\"{}🙂\"", "\\".repeat(16));
        let source: String = (1..=5_000)
            .map(|line| {
                if line <= 1_000 {
                    format!("// omitted {line}\n")
                } else if line % 3 == 0 {
                    format!("const needle_{line} = {literal};\n")
                } else {
                    format!("const value_{line} = {literal};\n")
                }
            })
            .collect();
        let path = root.write("views.ts", source);
        let policy = crate::tools::test_support::workspace_policy(&root.0);
        for &window in windows {
            for extra in [
                json!({"matchString":"needle", "contextLines":1}),
                json!({"minify":"standard", "unit":"lines"}),
                json!({"minify":"standard", "ranges":["1100-2800","3500-5000"]}),
            ] {
                let mut query = json!({"path":path});
                query
                    .as_object_mut()
                    .expect("object")
                    .extend(extra.as_object().expect("extra").clone());
                let (expected, _) = walk_local(query.clone(), &policy, None);
                let (actual, _) = walk_local(query, &policy, Some(window));
                assert!(!actual.is_empty());
                assert_eq!(actual, expected);
                assert_eq!(
                    actual
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len(),
                    actual.len()
                );
            }
        }
    }

    /// The default run walks one window; the full matrix (ignored: `cargo
    /// test -- --ignored full_matrix`) walks both.
    #[test]
    fn late_match_and_transformed_line_pages_keep_their_source_coordinates() {
        late_and_transformed_pages_keep_source_coordinates(&[20_000]);
    }

    #[test]
    #[ignore = "full matrix: cargo test -- --ignored full_matrix"]
    fn late_match_and_transformed_line_pages_keep_their_source_coordinates_full_matrix() {
        late_and_transformed_pages_keep_source_coordinates(&[20_000, 50_000]);
    }
}
