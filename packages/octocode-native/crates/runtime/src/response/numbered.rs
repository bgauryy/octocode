//! Numbered source content (docs/TOOL_DATA_CONTRACT.md).
//!
//! Hosts show agents the `structuredContent` JSON, so a read's `content`
//! carries its own source line numbers there, `cat -n`-like:
//! `<line>\t<text>` per line, no padding. Line-omission markers between
//! non-adjacent windows are separated by one `--` line.
//! Only views whose lines map one-to-one onto original source lines are
//! numbered; transformed views (`contentView`), byte windows, and content
//! whose lines do not align with `sourceLineRanges` stay verbatim.

use crate::id::ToolId;
use serde_json::Value;

/// Separator between the line number and the source text.
pub(crate) const SEPARATOR: char = '\t';

/// Append one gutter line: `<line>\t<record>` (`record` keeps its own
/// line ending, if any).
pub(crate) fn push_numbered(output: &mut String, line: impl std::fmt::Display, record: &str) {
    use std::fmt::Write as _;
    let _ = write!(output, "{line}{SEPARATOR}");
    output.push_str(record);
}

/// The line between two non-adjacent numbered windows (grep's group
/// separator): the numbers on either side state the lines it skips.
pub(crate) const GAP: &str = "--\n";

/// Number `content` whose lines are exactly the 1-based inclusive source
/// `ranges`, in order, with one line-omission marker between non-adjacent
/// windows. `None` when the content does not map onto the ranges.
#[must_use]
#[cfg(test)]
fn number_lines(content: &str, ranges: &[(u64, u64)]) -> Option<String> {
    number_lines_with_order(content, ranges, false)
}

fn number_lines_with_order(
    content: &str,
    ranges: &[(u64, u64)],
    reordered: bool,
) -> Option<String> {
    if ranges.is_empty() {
        return None;
    }
    let mut records: Vec<&str> = content.split_inclusive('\n').collect();
    // A page cut at a gap starts or ends on its marker: the gutter shows
    // the jump, so the edge marker carries nothing.
    while records
        .first()
        .is_some_and(|record| omission_span(record).is_some())
    {
        records.remove(0);
    }
    while records
        .last()
        .is_some_and(|record| omission_span(record).is_some())
    {
        records.pop();
    }
    let mut output = String::with_capacity(content.len() + records.len() * 6);
    let mut index = 0;
    let mut prev_end: Option<u64> = None;
    for &(start, end) in ranges {
        if start < 1 || end < start || (!reordered && prev_end.is_some_and(|prev| start <= prev)) {
            return None;
        }
        if (reordered || prev_end.is_some_and(|prev| start > prev + 1))
            && records
                .get(index)
                .is_some_and(|record| omission_span(record).is_some())
        {
            output.push_str(GAP);
            index += 1;
        }
        prev_end = Some(end);
        for line in start..=end {
            let record = records.get(index)?;
            // A byte-window marker is not a source line.
            if record.starts_with("... [") && record.contains(" omitted] ...") {
                return None;
            }
            push_numbered(&mut output, line, record);
            index += 1;
        }
    }
    (index == records.len()).then_some(output)
}

/// Why a span of source lines between two shown windows is not shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Omitted {
    /// The read did not ask for it (lines between ranges or match windows).
    NotRequested,
    /// A classified read did not select it.
    NotSelected,
}

impl Omitted {
    const fn words(self) -> &'static str {
        match self {
            Self::NotRequested => "not requested",
            Self::NotSelected => "not selected",
        }
    }
}

/// The one line-omission marker between two non-adjacent windows of a read
/// (`... [lines A-B not requested] ...`), so readers never mistake a gap for
/// contiguous source. [`omission_span`] reads back every kind.
pub(crate) fn omission_marker(start: usize, end: usize, why: Omitted) -> String {
    let words = why.words();
    if start == end {
        format!("... [line {start} {words}] ...\n")
    } else {
        format!("... [lines {start}-{end} {words}] ...\n")
    }
}

/// `(start, end)` of an [`omission_marker`] of any kind.
fn omission_span(record: &str) -> Option<(u64, u64)> {
    let inner = record
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("... [")?
        .strip_suffix("] ...")?;
    let inner = [Omitted::NotRequested, Omitted::NotSelected]
        .into_iter()
        .find_map(|why| inner.strip_suffix(why.words())?.strip_suffix(' '))?;
    if let Some(span) = inner.strip_prefix("lines ") {
        let (start, end) = span.split_once('-')?;
        Some((start.parse().ok()?, end.parse().ok()?))
    } else {
        let line = inner.strip_prefix("line ")?.parse().ok()?;
        Some((line, line))
    }
}

/// Whether `content` is already in numbered form: `N\t` lines in ascending
/// order, consecutive except across a [`GAP`] line, which stands for the
/// lines between its neighbours' numbers.
#[must_use]
fn is_numbered(content: &str) -> bool {
    let mut expected: Option<u64> = None;
    let mut gap = false;
    let mut numbered = 0usize;
    for record in content.split_inclusive('\n') {
        if record.trim_end_matches(['\n', '\r']) == GAP.trim_end() {
            if gap {
                return false;
            }
            gap = true;
            continue;
        }
        let Some((number, _)) = record.split_once(SEPARATOR) else {
            return false;
        };
        let Ok(line) = number.parse::<u64>() else {
            return false;
        };
        if line == 0 || number.starts_with('0') {
            return false;
        }
        match expected {
            Some(next) if gap && line <= next => return false,
            Some(next) if !gap && line != next => return false,
            _ => {}
        }
        gap = false;
        expected = Some(line + 1);
        numbered += 1;
    }
    numbered > 0
}

fn ranges_of(value: &Value) -> Option<Vec<(u64, u64)>> {
    value
        .as_array()?
        .iter()
        .map(|range| {
            Some((
                range.get("line")?.as_u64()?,
                range.get("endLine")?.as_u64()?,
            ))
        })
        .collect()
}

/// A transformed view, or a byte page: byte-page offsets count the returned
/// text, so it stays verbatim for exact reassembly.
fn is_transformed(data: &Value) -> bool {
    data.get("contentView")
        .and_then(Value::as_str)
        .is_some_and(|view| view != "none")
        || data.pointer("/pagination/unit").and_then(Value::as_str) == Some("bytes")
}

/// The numbered form of a file-read row's `content`: numbered now from its
/// `sourceLineRanges`, or as already numbered by the response stage. `None`
/// for a transformed or unaligned (verbatim) view.
#[must_use]
pub(crate) fn numbered_view(data: &Value) -> Option<String> {
    if is_transformed(data) {
        return None;
    }
    let content = data.get("content").and_then(Value::as_str)?;
    let reordered = ranked_read(data);
    match data.get("sourceLineRanges").and_then(ranges_of) {
        Some(ranges) => number_lines_with_order(content, &ranges, reordered),
        None => (is_numbered(content) || (reordered && is_numbered_reordered(content)))
            .then(|| content.to_owned()),
    }
}

fn ranked_read(data: &Value) -> bool {
    matches!(
        data.pointer("/selection/status").and_then(Value::as_str),
        Some("complete" | "partial")
    )
}

fn is_numbered_reordered(content: &str) -> bool {
    let mut seen = std::collections::HashSet::new();
    let mut count = 0;
    for record in content.split_inclusive('\n') {
        if record.trim_end_matches(['\n', '\r']) == GAP.trim_end() {
            continue;
        }
        let Some(line) = record
            .split_once(SEPARATOR)
            .and_then(|(number, _)| number.parse::<u64>().ok())
        else {
            return false;
        };
        if line == 0 || !seen.insert(line) {
            return false;
        }
        count += 1;
    }
    count > 0
}

/// Number one file-read row in place. The numbers state the source range, so
/// `sourceLineRanges` leaves the row; `matchedLines` leaves when every
/// returned line matched (a grep-style map).
fn number_file_row(data: &mut Value) -> bool {
    if is_transformed(data) {
        return false;
    }
    let Some(map) = data.as_object_mut() else {
        return false;
    };
    let reordered = matches!(
        map.get("selection")
            .and_then(|rank| rank.get("status"))
            .and_then(Value::as_str),
        Some("complete" | "partial")
    );
    let Some(ranges) = map.get("sourceLineRanges").and_then(ranges_of) else {
        return false;
    };
    let Some(numbered) = map
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| number_lines_with_order(content, &ranges, reordered))
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

/// A view that already carries its source gutter (the compact view, which
/// skips the lines it drops): every kept line states its number, in
/// ascending order. A byte page stays verbatim.
fn gutter_numbered(data: &Value) -> bool {
    if data.pointer("/pagination/unit").and_then(Value::as_str) == Some("bytes") {
        return false;
    }
    let Some(content) = data.get("content").and_then(Value::as_str) else {
        return false;
    };
    let mut last = 0u64;
    let mut numbered = false;
    for record in content.split_inclusive('\n') {
        if record.trim_end_matches(['\n', '\r']) == GAP.trim_end() {
            continue;
        }
        let Some(line) = record
            .split_once(SEPARATOR)
            .and_then(|(number, _)| number.parse::<u64>().ok())
            .filter(|line| *line > last)
        else {
            return false;
        };
        last = line;
        numbered = true;
    }
    numbered
}

/// Number every source-line read in a public `{results}` envelope:
/// localFetch and ghGetFileContent rows each carry one inline file.
pub(crate) fn number_read_rows(tool: ToolId, structured: &mut Value) {
    let rows = structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten();
    if tool.output().text_shape() != crate::response::output::TextShape::FileText {
        return;
    }
    for row in rows {
        if let Some(data) = row.get_mut("data")
            && (number_file_row(data) || gutter_numbered(data))
            && let Some(map) = data.as_object_mut()
        {
            // The gutter states the first, last and every returned
            // line, so the window fields repeat it.
            for key in ["startLine", "endLine", "returnedLines"] {
                map.remove(key);
            }
        }
    }
}

/// Source lines searched ahead of the last aligned one for a view line.
const VIEW_ALIGN_WINDOW: usize = 1_000;

/// Number a transformed view (`minify:"standard"`) with the source line each
/// of its lines came from, so every kept line can be cited. `source` is the
/// text the view was made from; `source_lines` gives each of its lines' 1-based
/// source number (`0` marks a line-omission marker), or `None` when `source`
/// starts at line 1. Lines align in order on their non-whitespace text: a view
/// line is the start of a source line's remaining text, else inside it. A
/// view line that joins several source lines, or aligns with none, has no
/// single citable number: `None`, and the caller keeps lines whole
/// ([`line_faithful_view`]). Omission markers stay unnumbered.
#[must_use]
pub(crate) fn number_view(
    source: &str,
    source_lines: Option<&[usize]>,
    view: &str,
) -> Option<String> {
    let squash = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let lines: Vec<(u64, String)> = source
        .lines()
        .enumerate()
        .filter_map(|(index, text)| {
            let line = match source_lines {
                Some(numbers) => *numbers.get(index)?,
                None => index + 1,
            };
            (line != 0).then(|| (line as u64, squash(text)))
        })
        .collect();
    if lines.is_empty() {
        return Some(view.to_owned());
    }
    let mut aligner = ViewAligner {
        lines: &lines,
        cur: 0,
        pos: 0,
        faithful: true,
    };
    let mut output = String::with_capacity(view.len() + view.len() / 8);
    for record in view.split_inclusive('\n') {
        if omission_span(record).is_some() {
            output.push_str(GAP);
            continue;
        }
        let number = aligner.align(&squash(record));
        if !aligner.faithful {
            return None;
        }
        push_numbered(&mut output, number, record);
    }
    Some(output)
}

/// The compact view when the transformed one is not line-faithful: every
/// source line whole under its own number, trailing whitespace trimmed and
/// blank lines left out (the gutter's jump shows them). Omission markers
/// stay unnumbered.
#[must_use]
pub(crate) fn line_faithful_view(source: &str, source_lines: Option<&[usize]>) -> String {
    let mut output = String::with_capacity(source.len());
    for (index, record) in source.lines().enumerate() {
        let line = match source_lines {
            Some(numbers) => numbers.get(index).copied().unwrap_or(0),
            None => index + 1,
        };
        if line == 0 {
            output.push_str(GAP);
            continue;
        }
        let text = record.trim_end();
        if text.trim().is_empty() {
            continue;
        }
        push_numbered(&mut output, line, text);
        output.push('\n');
    }
    output
}

/// In-order alignment state over squashed source lines: the current line and
/// the byte offset already matched in it.
struct ViewAligner<'a> {
    lines: &'a [(u64, String)],
    cur: usize,
    pos: usize,
    /// Every view line so far is one source line's text (or part of it).
    faithful: bool,
}

impl ViewAligner<'_> {
    /// The unmatched rest of line `index`.
    fn rest(&self, index: usize) -> (&str, usize) {
        let text = self.lines[index].1.as_str();
        if index == self.cur {
            (&text[self.pos.min(text.len())..], self.pos.min(text.len()))
        } else {
            (text, 0)
        }
    }

    fn current(&self) -> u64 {
        self.lines[self.cur.min(self.lines.len() - 1)].0
    }

    fn align(&mut self, key: &str) -> u64 {
        let end = self
            .cur
            .saturating_add(VIEW_ALIGN_WINDOW)
            .min(self.lines.len());
        if key.is_empty() {
            // A blank view line: the next blank source line.
            let start = if self.pos > 0 { self.cur + 1 } else { self.cur };
            if let Some(index) = (start..end).find(|&index| self.lines[index].1.is_empty()) {
                self.cur = index + 1;
                self.pos = 0;
                return self.lines[index].0;
            }
            return self.current();
        }
        for index in self.cur..end {
            let (rest, base) = self.rest(index);
            if rest.starts_with(key) {
                return self.take(index, base + key.len());
            }
        }
        for index in self.cur..end {
            let (rest, base) = self.rest(index);
            if let Some(at) = rest.find(key) {
                return self.take(index, base + at + key.len());
            }
        }
        for index in self.cur..end {
            let (rest, _) = self.rest(index);
            if rest.is_empty() || !key.starts_with(rest) {
                continue;
            }
            let mut matched = rest.len();
            for next in index + 1..self.lines.len() {
                let need = &key[matched..];
                let text = self.lines[next].1.as_str();
                if need.starts_with(text) {
                    matched += text.len();
                    if matched == key.len() {
                        let line = self.lines[index].0;
                        self.take(next, text.len());
                        self.faithful = false;
                        return line;
                    }
                } else if text.starts_with(need) {
                    let line = self.lines[index].0;
                    self.take(next, need.len());
                    self.faithful = false;
                    return line;
                } else {
                    break;
                }
            }
        }
        // No source line holds this text: its number would be a guess.
        self.faithful = false;
        self.current()
    }

    /// Consume line `index` up to byte `pos`; its number cites the view line.
    fn take(&mut self, index: usize, pos: usize) -> u64 {
        self.cur = index;
        self.pos = pos;
        self.lines[index].0
    }
}

/// One gutter for every outline: each `N<space>text`, `N\ttext` or `N}` line
/// becomes `N\ttext`. Lines without a leading number stay as they are.
#[must_use]
pub(crate) fn tab_gutter(outline: &str) -> String {
    let mut output = String::with_capacity(outline.len());
    for record in outline.split_inclusive('\n') {
        let digits = record.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || record.starts_with('0') || digits == record.trim_end().len() {
            output.push_str(record);
            continue;
        }
        let (number, rest) = record.split_at(digits);
        output.push_str(number);
        output.push(SEPARATOR);
        output.push_str(rest.strip_prefix([' ', SEPARATOR]).unwrap_or(rest));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transformed_views_cite_the_source_line_of_every_kept_line() {
        let source = "// c\n\nimport { a } from 'a';\nlet x = {a: 1, b: 2}; // t\n}\n";
        let view = "import{a}from 'a';\nlet x ={a:1,\nb:2};\n}\n";
        assert_eq!(
            number_view(source, None, view).as_deref(),
            Some("3\timport{a}from 'a';\n4\tlet x ={a:1,\n4\tb:2};\n5\t}\n")
        );
        // A window and its omission marker keep their own numbers.
        let window = "a();\n... [lines 3-8 not requested] ...\nb();\n";
        assert_eq!(
            number_view(
                window,
                Some(&[2, 0, 9]),
                "a();\n... [lines 3-8 not requested] ...\nb();\n"
            )
            .as_deref(),
            Some("2\ta();\n--\n9\tb();\n")
        );
        // Joined lines have no single citable number: the view is refused
        // and the line-faithful view keeps each line under its own number.
        assert_eq!(
            number_view("f(a,\n  b);\nc;\n", None, "f(a,b);\nc;\n"),
            None
        );
        assert_eq!(
            line_faithful_view("f(a,\n  b);  \n\nc;\n", None),
            "1\tf(a,\n2\t  b);\n4\tc;\n"
        );
        assert_eq!(
            line_faithful_view(
                "a();\n... [lines 3-8 not requested] ...\nb();\n",
                Some(&[2, 0, 9])
            ),
            "2\ta();\n--\n9\tb();\n"
        );
    }

    #[test]
    fn outlines_use_one_tab_gutter() {
        assert_eq!(
            tab_gutter("3 import x\n4\tfn a() {\n12}\nplain\n"),
            "3\timport x\n4\tfn a() {\n12\t}\nplain\n"
        );
    }

    #[test]
    fn numbers_each_line_with_a_tab_gutter_and_passes_markers_through() {
        assert_eq!(
            number_lines("a\nb\n", &[(4, 5)]).as_deref(),
            Some("4\ta\n5\tb\n")
        );
        assert_eq!(
            number_lines(
                "a\n... [lines 3-8 not requested] ...\nb\n",
                &[(2, 2), (9, 9)]
            )
            .as_deref(),
            Some("2\ta\n--\n9\tb\n")
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
        assert!(is_numbered("2\ta\n--\n9\tb\n"));
        assert!(is_numbered("7\t"));
        assert!(!is_numbered("4\ta\n6\tb\n"));
        assert!(!is_numbered("a\tb\n"));
        assert!(!is_numbered("0\ta\n"));
        assert!(!is_numbered(""));
        // A gap must skip at least one line, and gaps never stack.
        assert!(!is_numbered("2\ta\n--\n3\tb\n"));
        // A page cut at a gap still numbers: its edge markers drop.
        assert_eq!(
            number_lines(
                "... [lines 1-9 not requested] ...\nten\n... [lines 11-19 not requested] ...\ntwenty\n... [lines 21-29 not requested] ...\n",
                &[(10, 10), (20, 20)]
            )
            .as_deref(),
            Some("10\tten\n--\n20\ttwenty\n")
        );
        // The compact view skips the lines it drops: still a source gutter.
        let compact = |content: &str| gutter_numbered(&serde_json::json!({"content": content}));
        assert!(compact("4\ta\n9\tb\n--\n20\tc\n"));
        assert!(!compact("4\ta\n3\tb\n"));
        assert!(!compact("plain\n"));
        assert!(!is_numbered("2\ta\n--\n--\n9\tb\n"));
    }

    /// A grep map of single lines separates its windows with one `--` line
    /// each; the numbers state every skipped span, so the returned lines and
    /// ranges round-trip exactly.
    #[test]
    fn single_line_windows_use_one_gap_line_and_round_trip() {
        let ranges = [(3, 3), (10, 10), (20, 23), (40, 40), (50, 50)];
        let content = "c\n... [lines 4-9 not requested] ...\nj\n... [lines 11-19 not requested] ...\nt\nu\nv\nw\n... [lines 24-39 not requested] ...\nN\n... [lines 41-49 not requested] ...\nX\n";
        let numbered = number_lines(content, &ranges).expect("numbered");
        assert_eq!(
            numbered,
            "3\tc\n--\n10\tj\n--\n20\tt\n21\tu\n22\tv\n23\tw\n--\n40\tN\n--\n50\tX\n"
        );
        assert!(is_numbered(&numbered));
        let mut restored: Vec<(u64, u64)> = Vec::new();
        for record in numbered.lines() {
            let Some((number, _)) = record.split_once(SEPARATOR) else {
                continue;
            };
            let line: u64 = number.parse().expect("number");
            match restored.last_mut() {
                Some((_, end)) if *end + 1 == line => *end = line,
                _ => restored.push((line, line)),
            }
        }
        assert_eq!(restored, ranges);
    }

    /// A localFetch grep map (single-line matchString windows) gets one
    /// `--` line per gap between windows, and its numbers restore every line.
    #[test]
    fn local_fetch_single_line_windows_share_gap_markers_and_round_trip() {
        let mut local = json!({"results":[{"index":0,"data":{
            "path":"a.rs",
            "content":"c\n... [lines 4-9 not requested] ...\nj\n... [lines 11-19 not requested] ...\nt\n",
            "sourceLineRanges":[{"line":3,"endLine":3},{"line":10,"endLine":10},{"line":20,"endLine":20}],
            "matchedLines":[3,10,20]}}]});
        number_read_rows(ToolId::LocalFetch, &mut local);
        let content = local["results"][0]["data"]["content"]
            .as_str()
            .expect("content");
        assert_eq!(content, "3\tc\n--\n10\tj\n--\n20\tt\n");
        assert!(is_numbered(content));
        let lines = content
            .lines()
            .filter_map(|record| record.split_once(SEPARATOR))
            .map(|(number, _)| number.parse::<u64>().expect("number"))
            .collect::<Vec<_>>();
        assert_eq!(lines, [3, 10, 20]);
    }

    #[test]
    fn numbers_local_and_github_rows_and_drops_redundant_anchors() {
        let mut local = json!({"results":[{"index":0,"data":{
            "path":"a.rs","content":"x\ny\n","sourceLineRanges":[{"line":7,"endLine":8}],
            "matchedLines":[7,8]}}]});
        number_read_rows(ToolId::LocalFetch, &mut local);
        assert_eq!(
            local["results"][0]["data"],
            json!({"path":"a.rs","content":"7\tx\n8\ty\n"})
        );
        let mut window = json!({"results":[{"index":0,"data":{
            "content":"x\ny\n","sourceLineRanges":[{"line":7,"endLine":8}],"matchedLines":[8]}}]});
        number_read_rows(ToolId::LocalFetch, &mut window);
        assert_eq!(window["results"][0]["data"]["matchedLines"], json!([8]));
        // Two windows: the gutter states the first, last and returned lines.
        let mut windows = json!({"results":[{"index":0,"data":{
            "content":"x\n... [lines 8-9 not requested] ...\ny\n",
            "sourceLineRanges":[{"line":7,"endLine":7},{"line":10,"endLine":10}],
            "startLine":7,"endLine":10,"returnedLines":3,"totalLines":12}}]});
        number_read_rows(ToolId::LocalFetch, &mut windows);
        assert_eq!(
            windows["results"][0]["data"],
            json!({"content":"7\tx\n--\n10\ty\n","totalLines":12})
        );
        let mut remote = json!({"results":[
            {"index":0,"data":{"owner":"o","repo":"r","path":"a.py","content":"x\n",
             "sourceLineRanges":[{"line":3,"endLine":3}],"startLine":3,"endLine":3,"returnedLines":1}},
            {"index":1,"data":{"path":"b.py","content":"def a\n","contentView":"symbols"}}]});
        number_read_rows(ToolId::GhGetFileContent, &mut remote);
        assert_eq!(
            remote["results"][0]["data"],
            json!({"owner":"o","repo":"r","path":"a.py","content":"3\tx\n"}),
            "the gutter states the window"
        );
        assert_eq!(
            remote["results"][1]["data"]["content"], "def a\n",
            "transformed views stay verbatim"
        );
        // Byte pages stay verbatim: their offsets count the returned text.
        let mut bytes = json!({"results":[{"index":0,"data":{"content":"x\n",
            "sourceLineRanges":[{"line":1,"endLine":1}],
            "pagination":{"unit":"bytes","offset":0,"length":2,"hasMore":true}}}]});
        number_read_rows(ToolId::LocalFetch, &mut bytes);
        assert_eq!(bytes["results"][0]["data"]["content"], "x\n");
        // Other tools are untouched.
        let mut other = local.clone();
        number_read_rows(ToolId::LocalSearch, &mut other);
        assert_eq!(other, local);
    }

    /// Every omission marker kind reads back as its span, so a classified
    /// read whose selected chunks are not adjacent keeps its gutter.
    #[test]
    fn every_omission_marker_kind_keeps_the_gutter() {
        for why in [Omitted::NotRequested, Omitted::NotSelected] {
            assert_eq!(omission_span(&omission_marker(4, 4, why)), Some((4, 4)));
            assert_eq!(omission_span(&omission_marker(3, 8, why)), Some((3, 8)));
        }
        assert_eq!(omission_span("... [lines 3-8 not wanted] ...\n"), None);
        let content = format!(
            "x\n{}y\n",
            omission_marker(1153, 2538, Omitted::NotSelected)
        );
        let mut response = json!({"results":[{"data":{
            "content":content,
            "sourceLineRanges":[{"line":1152,"endLine":1152},{"line":2539,"endLine":2539}],
            "selection":{"status":"complete","selectedItems":2,"totalItems":3}
        }}]});
        number_read_rows(ToolId::LocalFetch, &mut response);
        assert_eq!(
            response["results"][0]["data"]["content"],
            "1152\tx\n--\n2539\ty\n"
        );
    }

    #[test]
    fn ranked_read_numbers_backwards_source_windows_and_keeps_gap_markers() {
        let mut response = serde_json::json!({"results":[{"data":{
            "content":"late\n... [lines 8-9 not requested] ...\nearlier\n",
            "sourceLineRanges":[{"line":10,"endLine":10},{"line":3,"endLine":3}],
            "selection":{"status":"complete","selectedItems":2,"totalItems":2}
        }}]});
        number_read_rows(ToolId::LocalFetch, &mut response);
        let data = &response["results"][0]["data"];
        assert_eq!(data["content"], "10\tlate\n--\n3\tearlier\n");
        assert_eq!(
            numbered_view(data).as_deref(),
            Some("10\tlate\n--\n3\tearlier\n")
        );
    }
}
