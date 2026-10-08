//! Numbered source content (docs/TOOL_DATA_CONTRACT.md).
//!
//! Hosts show agents the `structuredContent` JSON, so a read's `content`
//! carries its own source line numbers there, `cat -n`-like:
//! `<line>\t<text>` per line, no padding. Line-omission markers between
//! non-adjacent windows (`... [lines A-B not requested] ...`) stay unnumbered.
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
fn number_lines(content: &str, ranges: &[(u64, u64)]) -> Option<String> {
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

/// `(start, end)` of a `... [line N not requested] ...` / `... [lines A-B not requested] ...` marker.
fn omission_span(record: &str) -> Option<(u64, u64)> {
    let inner = record
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("... [")?
        .strip_suffix(" not requested] ...")?;
    if let Some(span) = inner.strip_prefix("lines ") {
        let (start, end) = span.split_once('-')?;
        Some((start.parse().ok()?, end.parse().ok()?))
    } else {
        let line = inner.strip_prefix("line ")?.parse().ok()?;
        Some((line, line))
    }
}

/// `(gaps, start, end)` of a gap-run marker
/// `... [N gaps in lines A-B not requested] ...`: N line-omission gaps, each
/// between two numbered lines, all inside lines A-B.
fn gap_run_span(record: &str) -> Option<(u64, u64, u64)> {
    let inner = record
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("... [")?
        .strip_suffix(" not requested] ...")?;
    let (gaps, span) = inner.split_once(" gaps in lines ")?;
    let (start, end) = span.split_once('-')?;
    Some((gaps.parse().ok()?, start.parse().ok()?, end.parse().ok()?))
}

/// Collapse each run of two or more line-omission markers that only single
/// numbered lines separate into one gap-run marker at the run's first gap
/// (`... [N gaps in lines A-B not requested] ...`). Every line keeps its number,
/// so each gap is still exactly the span between two numbers.
#[must_use]
fn collapse_gap_runs(numbered: &str) -> String {
    let records: Vec<&str> = numbered.split_inclusive('\n').collect();
    let mut output = String::with_capacity(numbered.len());
    let mut index = 0;
    while index < records.len() {
        let Some((start, mut end)) = omission_span(records[index]) else {
            output.push_str(records[index]);
            index += 1;
            continue;
        };
        // Extend over `single line, marker` pairs.
        let mut gaps = 1u64;
        let mut cursor = index + 1;
        while cursor + 1 < records.len()
            && records[cursor].ends_with('\n')
            && omission_span(records[cursor]).is_none()
            && let Some((_, next_end)) = omission_span(records[cursor + 1])
        {
            end = next_end;
            gaps += 1;
            cursor += 2;
        }
        if gaps < 2 {
            output.push_str(records[index]);
            index += 1;
            continue;
        }
        output.push_str(&format!(
            "... [{gaps} gaps in lines {start}-{end} not requested] ...\n"
        ));
        for line in (index + 1..cursor).step_by(2) {
            output.push_str(records[line]);
        }
        index = cursor;
    }
    output
}

/// Whether `content` is already in numbered form: consecutive `N\t` lines,
/// with only line-omission markers between runs, each skipping exactly the
/// lines it names, or gap-run markers whose lines jump exactly N times
/// inside the named span.
#[must_use]
fn is_numbered(content: &str) -> bool {
    let mut expected: Option<u64> = None;
    let mut numbered = 0usize;
    // An open gap run: gaps left to see, and the last omitted line.
    let mut run: Option<(u64, u64)> = None;
    for record in content.split_inclusive('\n') {
        if run.is_none()
            && let Some((gaps, start, end)) = gap_run_span(record)
        {
            if expected.is_some_and(|next| next != start) || end < start || gaps < 2 {
                return false;
            }
            run = Some((gaps, end));
            continue;
        }
        if let Some((start, end)) = omission_span(record) {
            if run.is_some() || expected.is_some_and(|next| next != start) || end < start {
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
        if line == 0 || number.starts_with('0') {
            return false;
        }
        match (expected, run) {
            (Some(next), Some((gaps, end))) if line > next => {
                // A jump is one gap of the run; the last one lands on end + 1.
                if gaps == 0 || line - 1 > end {
                    return false;
                }
                run = (gaps > 1).then_some((gaps - 1, end));
                if run.is_none() && line != end + 1 {
                    return false;
                }
            }
            (Some(next), _) if next != line => return false,
            _ => {}
        }
        expected = Some(line + 1);
        numbered += 1;
    }
    numbered > 0 && run.is_none()
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
fn number_file_row(data: &mut Value) -> bool {
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
/// localFetch and ghGetFileContent rows each carry one inline file.
pub fn number_read_rows(tool: ToolId, structured: &mut Value) {
    let rows = structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten();
    if !matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent) {
        return;
    }
    for row in rows {
        if let Some(data) = row.get_mut("data")
            && number_file_row(data)
            && let Some(map) = data.as_object_mut()
        {
            if let Some(Value::String(content)) = map.get_mut("content") {
                *content = collapse_gap_runs(content);
            }
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
/// line is the start of a source line's remaining text, else inside it, else
/// the start of a run of source lines it joins. Omission markers stay
/// unnumbered; a line with no alignment takes the current source line.
#[must_use]
pub fn number_view(source: &str, source_lines: Option<&[usize]>, view: &str) -> String {
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
        return view.to_owned();
    }
    let mut aligner = ViewAligner {
        lines: &lines,
        cur: 0,
        pos: 0,
    };
    let mut output = String::with_capacity(view.len() + view.len() / 8);
    for record in view.split_inclusive('\n') {
        if omission_span(record).is_some() || gap_run_span(record).is_some() {
            output.push_str(record);
            continue;
        }
        let number = aligner.align(&squash(record));
        output.push_str(&number.to_string());
        output.push(SEPARATOR);
        output.push_str(record);
    }
    output
}

/// In-order alignment state over squashed source lines: the current line and
/// the byte offset already matched in it.
struct ViewAligner<'a> {
    lines: &'a [(u64, String)],
    cur: usize,
    pos: usize,
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
                        return line;
                    }
                } else if text.starts_with(need) {
                    let line = self.lines[index].0;
                    self.take(next, need.len());
                    return line;
                } else {
                    break;
                }
            }
        }
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
pub fn tab_gutter(outline: &str) -> String {
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
            number_view(source, None, view),
            "3\timport{a}from 'a';\n4\tlet x ={a:1,\n4\tb:2};\n5\t}\n"
        );
        // A window and its omission marker keep their own numbers.
        let window = "a();\n... [lines 3-8 not requested] ...\nb();\n";
        assert_eq!(
            number_view(
                window,
                Some(&[2, 0, 9]),
                "a();\n... [lines 3-8 not requested] ...\nb();\n"
            ),
            "2\ta();\n... [lines 3-8 not requested] ...\n9\tb();\n"
        );
        // Joined lines cite their first line.
        assert_eq!(
            number_view("f(a,\n  b);\nc;\n", None, "f(a,b);\nc;\n"),
            "1\tf(a,b);\n3\tc;\n"
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
            Some("2\ta\n... [lines 3-8 not requested] ...\n9\tb\n")
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
        assert!(is_numbered(
            "2\ta\n... [lines 3-8 not requested] ...\n9\tb\n"
        ));
        assert!(is_numbered("7\t"));
        assert!(!is_numbered("4\ta\n6\tb\n"));
        assert!(!is_numbered("a\tb\n"));
        assert!(!is_numbered("0\ta\n"));
        assert!(!is_numbered(""));
        assert!(!is_numbered(
            "2\ta\n... [lines 4-8 not requested] ...\n9\tb\n"
        ));
    }

    /// A grep map of single lines carries one gap-run marker, not one marker
    /// per gap; every line keeps its number, so the omitted spans and the
    /// returned lines round-trip exactly.
    #[test]
    fn single_line_windows_share_one_gap_marker_and_round_trip() {
        let ranges = [(3, 3), (10, 10), (20, 23), (40, 40), (50, 50)];
        let content = "c\n... [lines 4-9 not requested] ...\nj\n... [lines 11-19 not requested] ...\nt\nu\nv\nw\n... [lines 24-39 not requested] ...\nN\n... [lines 41-49 not requested] ...\nX\n";
        let numbered = number_lines(content, &ranges).expect("numbered");
        let collapsed = collapse_gap_runs(&numbered);
        assert_eq!(
            collapsed,
            "3\tc\n... [2 gaps in lines 4-19 not requested] ...\n10\tj\n20\tt\n21\tu\n22\tv\n23\tw\n... [2 gaps in lines 24-49 not requested] ...\n40\tN\n50\tX\n"
        );
        assert!(collapsed.len() < numbered.len());
        assert!(is_numbered(&collapsed));
        // Round trip: the numbers restore every returned line and range.
        let mut restored: Vec<(u64, u64)> = Vec::new();
        for record in collapsed.lines() {
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
        // A lone gap keeps its own marker; nothing to collapse.
        let lone = number_lines(
            "a\n... [lines 3-8 not requested] ...\nb\n",
            &[(2, 2), (9, 9)],
        )
        .expect("lone");
        assert_eq!(collapse_gap_runs(&lone), lone);
        // A miscounted or misplaced run is not numbered content.
        assert!(!is_numbered(
            "3\tc\n... [3 gaps in lines 4-19 not requested] ...\n10\tj\n20\tt\n"
        ));
        assert!(!is_numbered(
            "3\tc\n... [2 gaps in lines 5-19 not requested] ...\n10\tj\n20\tt\n"
        ));
        assert!(!is_numbered(
            "3\tc\n... [2 gaps in lines 4-19 not requested] ...\n10\tj\n21\tt\n"
        ));
    }

    /// A localFetch grep map (single-line matchString windows) shares one
    /// gap-run marker per run of gaps, and its numbers restore every line.
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
        assert_eq!(
            content,
            "3\tc\n... [2 gaps in lines 4-19 not requested] ...\n10\tj\n20\tt\n"
        );
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
            json!({"content":"7\tx\n... [lines 8-9 not requested] ...\n10\ty\n","totalLines":12})
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
}
