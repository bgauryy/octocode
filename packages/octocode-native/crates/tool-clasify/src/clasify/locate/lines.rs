//! The file line of each line of a source's judged text. A selection of
//! `ranges` is read as the reader returns it: the ranges sorted and merged,
//! and between two windows the one `... [lines A-B not requested] ...`
//! marker line. A marker is no file line, so it has no number: the answers
//! name the file's lines around it, and a read that cites them never reads
//! the gap.
use octocode_runtime_base::response::numbered::{Omitted, omission_marker};
use octocode_tools_shared::line_spans::merge_spans;
use serde_json::Value;
use std::borrow::Cow;

/// The file line of each line of `text` (`None`: a gap marker between two
/// windows of `ranges`). Whole text is the file from line 1; a selection
/// holds its windows' lines in order, each gap a marker line that states
/// exactly the lines between its windows. `None` when the text is not that
/// selection (a range cut at the end of the file, a marker that states other
/// lines, lines left over), so no label could name a wrong line.
pub(super) fn file_lines(source: &Value, text: &str) -> Option<Vec<Option<usize>>> {
    let mut records = text.split_inclusive('\n');
    let windows = windows(source)?;
    if windows.is_empty() {
        return Some(
            records
                .enumerate()
                .map(|(index, _)| Some(index + 1))
                .collect(),
        );
    }
    let mut at = Vec::new();
    let mut previous_end = None;
    for (start, end) in windows {
        if let Some(previous) = previous_end {
            let gap = omission_marker(previous + 1, start - 1, Omitted::NotRequested);
            (records.next()? == gap).then_some(())?;
            at.push(None);
        }
        for line in start..=end {
            records.next()?;
            at.push(Some(line));
        }
        previous_end = Some(end);
    }
    records.next().is_none().then_some(at)
}

/// The windows `source`'s query reads, sorted and merged as the reader
/// merges them; none for a whole read. `None` for a range the reader
/// refuses.
fn windows(source: &Value) -> Option<Vec<(usize, usize)>> {
    let Some(ranges) = source["query"].get("ranges") else {
        return Some(Vec::new());
    };
    let mut windows = Vec::new();
    for range in ranges.as_array()? {
        let line = |key: &str| usize::try_from(range[key].as_u64()?).ok();
        let (start, end) = (line("startLine")?, line("endLine")?);
        if start == 0 || end < start {
            return None;
        }
        windows.push((start, end));
    }
    Some(merge_spans(windows))
}

/// A text without its gap markers, for cutting into parts: the lines that
/// are file lines, their numbers, and where each marker goes back.
pub(super) struct Gapped<'a> {
    pub text: Cow<'a, str>,
    /// The file line of each line of `text`.
    pub at: Vec<usize>,
    /// Each marker line and the index of the line of `text` it precedes.
    pub gaps: Vec<(usize, &'a str)>,
}

/// `text` split at `at` ([`file_lines`]): a text with no marker is borrowed
/// as it is.
pub(super) fn without_gaps<'a>(text: &'a str, at: &[Option<usize>]) -> Gapped<'a> {
    if !at.contains(&None) {
        return Gapped {
            text: Cow::Borrowed(text),
            at: at.iter().flatten().copied().collect(),
            gaps: Vec::new(),
        };
    }
    let mut lines = Vec::with_capacity(at.len());
    let mut gaps = Vec::new();
    let mut kept = String::with_capacity(text.len());
    for (record, line) in text.split_inclusive('\n').zip(at) {
        match line {
            Some(line) => {
                lines.push(*line);
                kept.push_str(record);
            }
            None => gaps.push((lines.len(), record)),
        }
    }
    Gapped {
        text: Cow::Owned(kept),
        at: lines,
        gaps,
    }
}
