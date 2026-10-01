//! Numbered source content (docs/TOOL_DATA_CONTRACT.md).
//!
//! Hosts show agents the `structuredContent` JSON, so a read's `content`
//! carries its own source line numbers there, `cat -n`-like:
//! `<line>\t<text>` per line, no padding. Line-omission markers between
//! non-adjacent windows (`... [lines A-B omitted] ...`) stay unnumbered.
//! Only views whose lines map one-to-one onto original source lines are
//! numbered; transformed views (`contentView`), byte windows, and content
//! whose lines do not align with `sourceLineRanges` stay verbatim.

use crate::tools::id::ToolId;
use serde_json::Value;

/// Separator between the line number and the source text.
pub const SEPARATOR: char = '\t';

/// Number `content` whose lines are exactly the 1-based inclusive source
/// `ranges`, in order, with one line-omission marker between non-adjacent
/// windows. `None` when the content does not map onto the ranges.
#[must_use]
pub fn number_lines(content: &str, ranges: &[(u64, u64)]) -> Option<String> {
    if ranges.is_empty() {
        return None;
    }
    let records: Vec<&str> = content.split_inclusive('\n').collect();
    let mut output = String::with_capacity(content.len() + records.len() * 6);
    let mut index = 0;
    let mut prev_end: Option<u64> = None;
    for &(start, end) in ranges {
        if start < 1 || end < start || prev_end.is_some_and(|prev| start <= prev) {
            return None;
        }
        if prev_end.is_some_and(|prev| start > prev + 1)
            && records
                .get(index)
                .is_some_and(|record| omission_span(record).is_some())
        {
            output.push_str(records[index]);
            index += 1;
        }
        prev_end = Some(end);
        for line in start..=end {
            let record = records.get(index)?;
            // A byte-window marker is not a source line.
            if record.starts_with("... [") && record.contains(" omitted] ...") {
                return None;
            }
            output.push_str(&line.to_string());
            output.push(SEPARATOR);
            output.push_str(record);
            index += 1;
        }
    }
    (index == records.len()).then_some(output)
}

/// `(start, end)` of a `... [line N omitted] ...` / `... [lines A-B omitted] ...` marker.
fn omission_span(record: &str) -> Option<(u64, u64)> {
    let inner = record
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("... [")?
        .strip_suffix(" omitted] ...")?;
    if let Some(span) = inner.strip_prefix("lines ") {
        let (start, end) = span.split_once('-')?;
        Some((start.parse().ok()?, end.parse().ok()?))
    } else {
        let line = inner.strip_prefix("line ")?.parse().ok()?;
        Some((line, line))
    }
}

/// Whether `content` is already in numbered form: consecutive `N\t` lines,
/// with only line-omission markers between runs, each skipping exactly the
/// lines it names.
#[must_use]
pub fn is_numbered(content: &str) -> bool {
    let mut expected: Option<u64> = None;
    let mut numbered = 0usize;
    for record in content.split_inclusive('\n') {
        if let Some((start, end)) = omission_span(record) {
            if expected.is_some_and(|next| next != start) || end < start {
                return false;
            }
            expected = Some(end + 1);
            continue;
        }
        let Some((number, _)) = record.split_once(SEPARATOR) else {
            return false;
        };
        let Ok(line) = number.parse::<u64>() else {
            return false;
        };
        if line == 0 || number.starts_with('0') || expected.is_some_and(|next| next != line) {
            return false;
        }
        expected = Some(line + 1);
        numbered += 1;
    }
    numbered > 0
}

fn ranges_of(value: &Value) -> Option<Vec<(u64, u64)>> {
    value
        .as_array()?
        .iter()
        .map(|range| Some((range.get("start")?.as_u64()?, range.get("end")?.as_u64()?)))
        .collect()
}

/// A transformed view, or a byte page: byte-page offsets count the returned
/// text, so it stays verbatim for exact reassembly.
fn is_transformed(data: &Value) -> bool {
    data.get("contentView")
        .and_then(Value::as_str)
        .is_some_and(|view| view != "none")
        || data
            .pointer("/pagination/chunkType")
            .and_then(Value::as_str)
            == Some("bytes")
}

/// The numbered form of a file-read row's `content`: numbered now from its
/// `sourceLineRanges`, or as already numbered by the response stage. `None`
/// for a transformed or unaligned (verbatim) view.
#[must_use]
pub fn numbered_view(data: &Value) -> Option<String> {
    if is_transformed(data) {
        return None;
    }
    let content = data.get("content").and_then(Value::as_str)?;
    match data.get("sourceLineRanges").and_then(ranges_of) {
        Some(ranges) => number_lines(content, &ranges),
        None => is_numbered(content).then(|| content.to_owned()),
    }
}

/// Number one file-read row in place. The numbers state the source range, so
/// `sourceLineRanges` leaves the row; `matchedLines` leaves when every
/// returned line matched (a grep-style map).
pub fn number_file_row(data: &mut Value) -> bool {
    if is_transformed(data) {
        return false;
    }
    let Some(map) = data.as_object_mut() else {
        return false;
    };
    let Some(ranges) = map.get("sourceLineRanges").and_then(ranges_of) else {
        return false;
    };
    let Some(numbered) = map
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| number_lines(content, &ranges))
    else {
        return false;
    };
    let returned: u64 = ranges.iter().map(|(start, end)| end + 1 - start).sum();
    let all_matched = map
        .get("matchedLines")
        .and_then(Value::as_array)
        .is_some_and(|lines| lines.len() as u64 == returned);
    if all_matched {
        map.remove("matchedLines");
    }
    map.insert("content".into(), Value::String(numbered));
    map.remove("sourceLineRanges");
    true
}

/// Number every source-line read in a public `{results}` envelope:
/// localFetch rows carry one inline file, ghGetFileContent rows a `files[]`.
pub fn number_read_rows(tool: ToolId, structured: &mut Value) {
    let rows = structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten();
    match tool {
        ToolId::LocalFetch => {
            for row in rows {
                if let Some(data) = row.get_mut("data") {
                    number_file_row(data);
                }
            }
        }
        ToolId::GhGetFileContent => {
            for file in rows.filter_map(|row| {
                row.get_mut("data")
                    .and_then(|data| data.get_mut("files"))
                    .and_then(Value::as_array_mut)
            }) {
                for file in file {
                    number_file_row(file);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_each_line_with_a_tab_gutter_and_passes_markers_through() {
        assert_eq!(
            number_lines("a\nb\n", &[(4, 5)]).as_deref(),
            Some("4\ta\n5\tb\n")
        );
        assert_eq!(
            number_lines("a\n... [lines 3-8 omitted] ...\nb\n", &[(2, 2), (9, 9)]).as_deref(),
            Some("2\ta\n... [lines 3-8 omitted] ...\n9\tb\n")
        );
        // Last line without a trailing newline keeps that shape.
        assert_eq!(
            number_lines("x\ny", &[(10, 11)]).as_deref(),
            Some("10\tx\n11\ty")
        );
    }

    #[test]
    fn rejects_content_that_does_not_map_onto_the_ranges() {
        assert_eq!(number_lines("a\nb\n", &[(4, 4)]), None);
        assert_eq!(number_lines("a\n", &[(0, 1)]), None);
        assert_eq!(number_lines("a\nb\nc\n", &[(1, 5)]), None);
        assert_eq!(number_lines("", &[(1, 1)]), None);
        assert_eq!(number_lines("a\n", &[]), None);
        // Byte windows are not source lines.
        assert_eq!(
            number_lines("xx\n... [3000 bytes omitted] ...\n", &[(1, 2)]),
            None
        );
    }

    #[test]
    fn recognizes_numbered_content_only() {
        assert!(is_numbered("4\ta\n5\tb\n"));
        assert!(is_numbered("2\ta\n... [lines 3-8 omitted] ...\n9\tb\n"));
        assert!(is_numbered("7\t"));
        assert!(!is_numbered("4\ta\n6\tb\n"));
        assert!(!is_numbered("a\tb\n"));
        assert!(!is_numbered("0\ta\n"));
        assert!(!is_numbered(""));
        assert!(!is_numbered("2\ta\n... [lines 4-8 omitted] ...\n9\tb\n"));
    }

    #[test]
    fn numbers_local_and_github_rows_and_drops_redundant_anchors() {
        let mut local = json!({"results":[{"index":0,"data":{
            "path":"a.rs","content":"x\ny\n","sourceLineRanges":[{"start":7,"end":8}],
            "matchedLines":[7,8]}}]});
        number_read_rows(ToolId::LocalFetch, &mut local);
        assert_eq!(
            local["results"][0]["data"],
            json!({"path":"a.rs","content":"7\tx\n8\ty\n"})
        );
        let mut window = json!({"results":[{"index":0,"data":{
            "content":"x\ny\n","sourceLineRanges":[{"start":7,"end":8}],"matchedLines":[8]}}]});
        number_read_rows(ToolId::LocalFetch, &mut window);
        assert_eq!(window["results"][0]["data"]["matchedLines"], json!([8]));
        let mut remote = json!({"results":[{"index":0,"data":{"owner":"o","repo":"r","files":[
            {"path":"a.py","content":"x\n","sourceLineRanges":[{"start":3,"end":3}]},
            {"path":"b.py","content":"def a\n","contentView":"symbols"}]}}]});
        number_read_rows(ToolId::GhGetFileContent, &mut remote);
        assert_eq!(
            remote["results"][0]["data"]["files"][0]["content"],
            "3\tx\n"
        );
        assert_eq!(
            remote["results"][0]["data"]["files"][1]["content"], "def a\n",
            "transformed views stay verbatim"
        );
        // Byte pages stay verbatim: their offsets count the returned text.
        let mut bytes = json!({"results":[{"index":0,"data":{"content":"x\n",
            "sourceLineRanges":[{"start":1,"end":1}],
            "pagination":{"chunkType":"bytes","offset":0,"chunkSize":2,"hasMore":true}}}]});
        number_read_rows(ToolId::LocalFetch, &mut bytes);
        assert_eq!(bytes["results"][0]["data"]["content"], "x\n");
        // Other tools are untouched.
        let mut other = local.clone();
        number_read_rows(ToolId::LocalSearch, &mut other);
        assert_eq!(other, local);
    }
}
