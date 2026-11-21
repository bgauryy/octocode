//! Numbered source content (docs/TOOL_DATA_CONTRACT.md).
//!
//! Agents read a read's `content` as the response gives it, so it carries
//! its own source line numbers, `cat -n`-like:
//! `<line>\t<text>` per line, no padding. Line-omission markers between
//! non-adjacent windows are separated by one `--` line.
//! Only views whose lines map one-to-one onto original source lines are
//! numbered; transformed views (`contentView`) and content whose lines do
//! not align with `sourceLineRanges` stay verbatim. A byte page (a window
//! of a line too long for one page) is numbered the same way, so a walk
//! keeps one format: each row that starts a source line carries its
//! gutter, and a page that opens inside a line states `continuesLine` and
//! leaves that first row bare. Byte offsets count the source text, never
//! the gutters. A byte page of match windows joined by byte-gap markers
//! does not map onto lines and stays verbatim beside its
//! `sourceLineRanges`.

mod view;
use crate::id::ToolId;
use serde_json::Value;
pub use view::tab_gutter;

/// Separator between the line number and the source text.
pub const SEPARATOR: char = '\t';

/// Append one gutter line: `<line>\t<record>` (`record` keeps its own
/// line ending, if any).
pub fn push_numbered(output: &mut String, line: impl std::fmt::Display, record: &str) {
    use std::fmt::Write as _;
    let _ = write!(output, "{line}{SEPARATOR}");
    output.push_str(record);
}

/// The line between two non-adjacent numbered windows (grep's group
/// separator): the numbers on either side state the lines it skips.
pub const GAP: &str = "--\n";

/// Number `content` whose lines are exactly the 1-based inclusive source
/// `ranges`, in order, with one line-omission marker between non-adjacent
/// windows. `None` when the content does not map onto the ranges.
#[must_use]
#[cfg(test)]
fn number_lines(content: &str, ranges: &[(u64, u64)]) -> Option<String> {
    number_lines_with_order(content, ranges, false, false)
}

/// `continues`: the content opens inside its first line, whose first row is
/// a later piece of it and takes no gutter.
fn number_lines_with_order(
    content: &str,
    ranges: &[(u64, u64)],
    reordered: bool,
    continues: bool,
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
            if continues && index == 0 {
                output.push_str(record);
            } else {
                push_numbered(&mut output, line, record);
            }
            index += 1;
        }
    }
    (index == records.len()).then_some(output)
}

/// Why a span of source lines between two shown windows is not shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Omitted {
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
pub fn omission_marker(start: usize, end: usize, why: Omitted) -> String {
    let words = why.words();
    if start == end {
        format!("... [line {start} {words}] ...\n")
    } else {
        format!("... [lines {start}-{end} {words}] ...\n")
    }
}

/// Whether `record` is a line-omission marker or a [`GAP`]: a separator
/// between windows, not a source row.
pub fn is_separator(record: &str) -> bool {
    record == GAP || omission_span(record).is_some()
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

/// A transformed view: its lines are not the source's.
fn is_transformed(data: &Value) -> bool {
    data.get("contentView")
        .and_then(Value::as_str)
        .is_some_and(|view| view != "none")
}

/// The line a byte page's content opens inside (`continuesLine`).
fn continued_line(data: &Value) -> Option<u64> {
    data.get("continuesLine").and_then(Value::as_u64)
}

/// Number one file-read row in place. The numbers replace `sourceLineRanges`;
/// `matchedLines` still distinguishes hits from returned context.
fn number_file_row(data: &mut Value) -> bool {
    if is_transformed(data) {
        return false;
    }
    let continues = continued_line(data);
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
    // The line a page continues is the first it shows.
    if continues.is_some_and(|line| ranges.first().map(|range| range.0) != Some(line)) {
        return false;
    }
    let Some(numbered) = map
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| {
            number_lines_with_order(content, &ranges, reordered, continues.is_some())
        })
    else {
        return false;
    };
    map.insert("content".into(), Value::String(numbered));
    map.remove("sourceLineRanges");
    true
}

/// A view that already carries its source gutter (the compact view, which
/// skips the lines it drops; the symbols outline, whose declarations state
/// their span `N-M`): every returned line states its number, in source
/// order (only an `N-M` span may start on the previous line: an outline's
/// nested declaration starts on its parent's line).
/// A byte page stays verbatim.
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
            .and_then(|(number, _)| gutter_line(number))
            .filter(|(line, span)| *line > last || (*span && *line == last && last > 0))
            .map(|(line, _)| line)
        else {
            return false;
        };
        last = line;
        numbered = true;
    }
    numbered
}

/// The first source line a gutter states, and whether it is a span:
/// `N`, or `N-M` (`M >= N`).
fn gutter_line(number: &str) -> Option<(u64, bool)> {
    match number.split_once('-') {
        Some((start, end)) => {
            let (start, end) = (start.parse::<u64>().ok()?, end.parse::<u64>().ok()?);
            (end >= start).then_some((start, true))
        }
        None => number.parse().ok().map(|line| (line, false)),
    }
}

/// Number every source-line read in a public `{results}` envelope:
/// localFetch and ghGetFileContent rows each carry one inline file.
pub fn number_read_rows(tool: ToolId, structured: &mut Value) {
    let rows = structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten();
    if !tool.reads_source() {
        return;
    }
    for row in rows {
        if let Some(data) = row.get_mut("data") {
            number_read_row(data);
        }
    }
}

/// Number one source-line read's row `data` in place, as
/// [`number_read_rows`] numbers each row of a source reader.
pub(crate) fn number_read_row(data: &mut Value) {
    if (number_file_row(data) || gutter_numbered(data))
        && let Some(map) = data.as_object_mut()
    {
        // The gutter states the first, last and every returned
        // line, so the window fields repeat it.
        for key in ["startLine", "endLine", "returnedLines"] {
            map.remove(key);
        }
    }
}

#[cfg(test)]
mod tests;
