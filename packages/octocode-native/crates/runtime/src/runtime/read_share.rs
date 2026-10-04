//! Batched source reads share one response window.
//!
//! The response pager moves whole rows to later response pages when a batch
//! overflows the automatic window, and a read row is one `content` string it
//! cannot divide: a three-file batch then shows one file and hides two behind
//! `responsePagination.next`. Before the rows are shaped, this step finds a
//! `localFetch` or `ghGetFileContent` batch that would overflow, gives each
//! row a fair share of the window (a small row keeps its size and leaves the
//! rest to larger ones), and re-reads each oversized row as a shorter line
//! page. Every row then returns its first lines plus its own `next.continue`
//! for exactly the rest; nothing is dropped.

use serde_json::{Map, Value};

use super::ExecutionError;
use super::channels::PAGES_KEY;
use super::dispatch::DomainResult;
use crate::response::fair_shares;
use crate::tools::id::ToolId;
use crate::tools::stream_page::{json_chars, json_text_chars};

/// Room for the envelope around the rows: `base`, `shared`, and the
/// response-level warning that names the partial rows.
const ENVELOPE_CHARS: usize = 512;

/// Room for a shortened row's `next.continue` beyond the query it copies:
/// tool name, offset, chunk type, and the row's remaining-lines warning.
const CONTINUATION_EXTRA_CHARS: usize = 192;

/// Tools whose rows each carry one source `content` string.
fn reads_source(tool: ToolId) -> bool {
    matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent)
}

/// The object holding a read row's `content`, `pagination`, and `next`: the
/// row itself for `localFetch`, its one file for `ghGetFileContent`.
fn file(tool: ToolId, data: &Value) -> Option<&Map<String, Value>> {
    match tool {
        ToolId::GhGetFileContent => data.get("files")?.as_array()?.first()?.as_object(),
        _ => data.as_object(),
    }
}

fn file_mut(tool: ToolId, data: &mut Value) -> Option<&mut Map<String, Value>> {
    match tool {
        ToolId::GhGetFileContent => data
            .get_mut("files")?
            .as_array_mut()?
            .first_mut()?
            .as_object_mut(),
        _ => data.as_object_mut(),
    }
}

/// Chars the `<line>\t` gutter adds to each content line once the response
/// numbers it: the line number's digits plus the JSON-escaped tab.
fn gutter_chars(file: &Map<String, Value>) -> usize {
    let last = file.get("totalLines").and_then(Value::as_u64).unwrap_or(0);
    last.max(1).to_string().len() + 2
}

/// Chars one content line takes in the response.
fn line_chars(line: &str, gutter: usize) -> usize {
    json_text_chars(line) + gutter
}

/// The row's `content`, when the row is a successful read with text.
fn content(tool: ToolId, row: &DomainResult) -> Option<(&str, usize)> {
    if row.failure.is_some() || row.status.is_some() {
        return None;
    }
    let file = file(tool, &row.data)?;
    let text = file.get("content")?.as_str()?;
    (!text.is_empty()).then(|| (text, gutter_chars(file)))
}

/// Estimated response chars of a row: its JSON plus the line-number gutter.
fn row_chars(tool: ToolId, row: &DomainResult) -> usize {
    let gutter = file(tool, &row.data)
        .and_then(|file| file.get("content")?.as_str().map(|text| (text, gutter_chars(file))))
        .map_or(0, |(text, gutter)| text.split_inclusive('\n').count() * gutter);
    json_chars(&row.data) + gutter
}

/// A row whose query pages its view by lines from where this page started:
/// no caller-chosen page size, and no byte offset to keep.
fn pages_by_lines(query: &Value) -> bool {
    if query.get("chunkSize").is_some() {
        return false;
    }
    let lines = match query.get("chunkType").and_then(Value::as_str) {
        Some(kind) => kind == "lines",
        // A whole-file read pages by bytes unless told otherwise; from its
        // start, its first line page is the same text.
        None => query.get("fullContent") != Some(&Value::Bool(true)),
    };
    lines || query.get("offset").and_then(Value::as_u64).unwrap_or(0) == 0
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

/// Line-page sizes for the rows of one batch that overflow `window`: row
/// index to the lines its shorter page keeps.
fn plan(tool: ToolId, queries: &[Value], rows: &[DomainResult], window: usize) -> Vec<(usize, usize)> {
    if !reads_source(tool) || rows.len() < 2 || rows.len() != queries.len() {
        return Vec::new();
    }
    let sizes: Vec<usize> = rows.iter().map(|row| row_chars(tool, row)).collect();
    if sizes.iter().sum::<usize>() + ENVELOPE_CHARS <= window {
        return Vec::new();
    }
    let shares = fair_shares(&sizes, window.saturating_sub(ENVELOPE_CHARS));
    let mut cuts = Vec::new();
    for (index, (row, query)) in rows.iter().zip(queries).enumerate() {
        if sizes[index] <= shares[index] || !pages_by_lines(query) {
            continue;
        }
        let Some((text, gutter)) = content(tool, row) else {
            continue;
        };
        let text_chars: usize = text
            .split_inclusive('\n')
            .map(|line| line_chars(line, gutter))
            .sum();
        let around = sizes[index].saturating_sub(text_chars)
            + json_chars(query)
            + CONTINUATION_EXTRA_CHARS;
        let keep = lines_within(text, gutter, shares[index].saturating_sub(around));
        if keep < text.split_inclusive('\n').count() {
            cuts.push((index, keep));
        }
    }
    cuts
}

/// The shorter page's continuations read the rest at the default page size,
/// not at this row's share of a batch.
fn drop_share_page_size(tool: ToolId, data: &mut Value) {
    let Some(Value::Object(next)) = file_mut(tool, data).and_then(|file| file.get_mut(PAGES_KEY))
    else {
        return;
    };
    for call in next.values_mut() {
        if let Some(Value::Object(query)) = call.get_mut("query") {
            query.remove("chunkSize");
        }
    }
}

/// Re-read each row of an overflowing `localFetch`/`ghGetFileContent` batch
/// that exceeds its fair share of `window` as a shorter line page. `rerun`
/// executes the shortened queries (in order) and returns one row each; a
/// shortened read that fails keeps the row's first result.
pub(super) fn share_window(
    tool: ToolId,
    queries: &[Value],
    mut rows: Vec<DomainResult>,
    window: Option<usize>,
    rerun: impl FnOnce(&[Value]) -> Result<Vec<DomainResult>, ExecutionError>,
) -> Result<Vec<DomainResult>, ExecutionError> {
    let Some(window) = window.filter(|window| *window > 0) else {
        return Ok(rows);
    };
    let cuts = plan(tool, queries, &rows, window);
    if cuts.is_empty() {
        return Ok(rows);
    }
    let shortened: Vec<Value> = cuts
        .iter()
        .map(|&(index, lines)| {
            let mut query = queries[index].clone();
            // A whole-file read pages as the same view without
            // `fullContent`, which chunk controls cannot accompany.
            if let Some(fields) = query.as_object_mut()
                && fields.remove("fullContent") == Some(Value::Bool(true))
            {
                fields.insert("offset".into(), Value::from(0));
            }
            query["chunkType"] = Value::from("lines");
            query["chunkSize"] = Value::from(lines);
            query
        })
        .collect();
    let reread = rerun(&shortened)?;
    for (&(index, _), mut row) in cuts.iter().zip(reread) {
        if content(tool, &row).is_none() {
            continue;
        }
        drop_share_page_size(tool, &mut row.data);
        rows[index] = row;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn local_row(lines: usize, width: usize) -> DomainResult {
        let text: String = (1..=lines)
            .map(|line| format!("{line:05}{}\n", "x".repeat(width)))
            .collect();
        DomainResult::payload(json!({"path":"a.txt","content":text,"totalLines":lines}), None)
    }

    fn query(extra: Value) -> Value {
        let mut query = json!({"path":"a.txt"});
        for (key, value) in extra.as_object().into_iter().flatten() {
            query[key] = value.clone();
        }
        query
    }

    #[test]
    fn a_batch_that_fits_is_left_alone() {
        let rows = vec![local_row(10, 20), local_row(10, 20)];
        let queries = vec![query(json!({})), query(json!({}))];
        assert!(plan(ToolId::LocalFetch, &queries, &rows, 20_000).is_empty());
    }

    #[test]
    fn overflowing_rows_get_fair_line_pages_and_small_rows_keep_theirs() {
        let rows = vec![local_row(5, 20), local_row(300, 60), local_row(300, 60)];
        let queries = vec![query(json!({})), query(json!({})), query(json!({}))];
        let cuts = plan(ToolId::LocalFetch, &queries, &rows, 20_000);
        assert_eq!(cuts.iter().map(|cut| cut.0).collect::<Vec<_>>(), vec![1, 2]);
        for (_, lines) in &cuts {
            assert!((100..300).contains(lines), "{cuts:?}");
        }
    }

    #[test]
    fn caller_chosen_pages_and_byte_offsets_are_not_reshaped() {
        let rows = vec![local_row(300, 60), local_row(300, 60), local_row(300, 60)];
        let queries = vec![
            query(json!({"chunkSize": 300})),
            query(json!({"fullContent": true, "offset": 4096})),
            query(json!({"chunkType": "bytes", "offset": 10})),
        ];
        assert!(plan(ToolId::LocalFetch, &queries, &rows, 20_000).is_empty());
    }

    #[test]
    fn single_rows_and_other_tools_are_never_reshaped() {
        let rows = vec![local_row(900, 60)];
        assert!(plan(ToolId::LocalFetch, &[query(json!({}))], &rows, 20_000).is_empty());
        let rows = vec![local_row(300, 60), local_row(300, 60)];
        let queries = vec![query(json!({})), query(json!({}))];
        assert!(plan(ToolId::LocalSearch, &queries, &rows, 20_000).is_empty());
    }

    #[test]
    fn shortened_rows_replace_the_first_read_and_continue_at_the_default_page() {
        let rows = vec![local_row(300, 60), local_row(300, 60)];
        let queries = vec![query(json!({})), query(json!({}))];
        let mut seen = Vec::new();
        let shared = share_window(ToolId::LocalFetch, &queries, rows, Some(20_000), |shortened| {
            seen = shortened.to_vec();
            Ok(shortened
                .iter()
                .map(|query| {
                    let lines = query["chunkSize"].as_u64().unwrap_or(0) as usize;
                    let mut row = local_row(lines, 60);
                    row.data["next"] = json!({"continue":{"tool":"localFetch","query":{
                        "path":"a.txt","offset":lines,"chunkType":"lines","chunkSize":lines}}});
                    row
                })
                .collect())
        })
        .expect("shared");
        assert_eq!(seen.len(), 2);
        assert!(seen.iter().all(|query| query["chunkType"] == "lines"));
        for row in &shared {
            let next = &row.data["next"]["continue"]["query"];
            assert!(next.get("chunkSize").is_none(), "{next}");
            assert!(next["offset"].as_u64().is_some_and(|offset| offset > 0));
        }
        let total: usize = shared.iter().map(|row| row_chars(ToolId::LocalFetch, row)).sum();
        assert!(total + ENVELOPE_CHARS <= 20_000, "{total}");
    }

    #[test]
    fn a_failed_shortened_read_keeps_the_first_result() {
        let rows = vec![local_row(300, 60), local_row(300, 60)];
        let queries = vec![query(json!({})), query(json!({}))];
        let shared = share_window(ToolId::LocalFetch, &queries, rows, Some(20_000), |shortened| {
            Ok(shortened
                .iter()
                .map(|_| DomainResult::payload(json!({"error":"gone"}), Some("error")))
                .collect())
        })
        .expect("shared");
        assert!(shared.iter().all(|row| row.data["content"].is_string()));
    }

    #[test]
    fn github_rows_are_measured_and_reshaped_through_their_file() {
        let file = |lines: usize| {
            let text: String = (1..=lines).map(|line| format!("{line:05}{}\n", "y".repeat(60))).collect();
            DomainResult::payload(
                json!({"owner":"o","repo":"r","files":[{"path":"a.rs","content":text,"totalLines":lines}]}),
                None,
            )
        };
        let rows = vec![file(300), file(300), file(4)];
        let queries = vec![
            json!({"owner":"o","repo":"r","path":"a.rs","fullContent":true}),
            json!({"owner":"o","repo":"r","path":"b.rs","fullContent":true}),
            json!({"owner":"o","repo":"r","path":"c.rs","fullContent":true}),
        ];
        let cuts = plan(ToolId::GhGetFileContent, &queries, &rows, 20_000);
        assert_eq!(cuts.iter().map(|cut| cut.0).collect::<Vec<_>>(), vec![0, 1]);
    }
}
