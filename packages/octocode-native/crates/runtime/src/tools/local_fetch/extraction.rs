use super::types::{LineRange, LocalFetchQuery, RegexMatch};

pub struct Extraction {
    pub text: String,
    pub source_lines: Option<Vec<usize>>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub match_ranges: Vec<LineRange>,
    pub matched_lines: Vec<usize>,
    pub count: Option<usize>,
    pub warnings: Vec<String>,
}
/// A matched line longer than this (minified or bundled source) is read in
/// byte windows unless the caller chose a context.
const LONG_LINE_BYTES: usize = 2_000;
const LONG_LINE_CONTEXT_BYTES: usize = 200;

/// View-line sentinel for an omission marker (source lines are 1-based).
pub const OMISSION_LINE: usize = 0;

/// Separates non-adjacent match windows so readers never mistake a gap for
/// contiguous source.
pub fn omission_marker(start: usize, end: usize) -> String {
    if start == end {
        format!("... [line {start} omitted] ...\n")
    } else {
        format!("... [lines {start}-{end} omitted] ...\n")
    }
}
fn records(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '\n' {
            out.push(&s[start..=i]);
            start = i + 1
        }
    }
    if start < s.len() {
        out.push(&s[start..])
    }
    out
}
pub fn line_count(s: &str) -> usize {
    records(s).len()
}
pub fn extract(
    q: &LocalFetchQuery,
    content: &str,
    regex: &impl RegexMatch,
) -> Result<Extraction, String> {
    let lines = records(content);
    let total = lines.len();
    if let Some(pattern) = q.match_string.as_ref() {
        return match_extract(q, content, &lines, pattern, regex);
    }
    if let (Some(start), Some(end)) = (q.start_line(), q.end_line()) {
        if end < start {
            return Err(format!(
                "startLine {start} is greater than endLine {end} — startLine must be ≤ endLine"
            ));
        }
        if start > total {
            return Err(format!(
                "Requested startLine {start} exceeds file length ({total} lines)"
            ));
        }
        let end2 = end.min(total);
        let selected = &lines[start.saturating_sub(1)..end2];
        return Ok(Extraction {
            text: selected.concat(),
            source_lines: Some((start..=end2).collect()),
            start: Some(start.max(1)),
            end: Some(end2),
            match_ranges: vec![],
            matched_lines: vec![],
            count: None,
            warnings: if end > total {
                vec![format!(
                    "Requested endLine {end} adjusted to {total} (file end)"
                )]
            } else {
                vec![]
            },
        });
    }
    Ok(Extraction {
        text: content.into(),
        source_lines: None,
        start: None,
        end: None,
        match_ranges: vec![],
        matched_lines: vec![],
        count: None,
        warnings: vec![],
    })
}
fn match_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    pattern: &str,
    regex: &impl RegexMatch,
) -> Result<Extraction, String> {
    let sensitive = q.match_string_case_sensitive.unwrap_or(false);
    let spans = if q.match_string_is_regex.unwrap_or(false) {
        regex.matching_ranges(pattern, sensitive, content).map_err(|error| {
            if pattern == "[" { "Invalid regex pattern: Invalid regular expression: /[/: Unterminated character class".to_owned() } else { error }
        })?
    } else {
        literal_ranges(content, pattern, sensitive)
    };
    let mut hits = vec![];
    let mut line_start = 0;
    let mut next_match = 0;
    for (i, line) in lines.iter().enumerate() {
        let line_end = line_start + line.len();
        while spans
            .get(next_match)
            .is_some_and(|(start, end)| *start < line_start && *end <= line_start)
        {
            next_match += 1;
        }
        // Include every line touched by a multiline match, not just its start.
        // A match ending at the next line's start does not touch that line.
        let found = spans
            .get(next_match)
            .is_some_and(|(start, _)| *start < line_end);
        if found {
            hits.push(i + 1)
        }
        line_start = line_end;
    }
    if hits.is_empty() {
        return Ok(Extraction {
            text: String::new(),
            source_lines: Some(vec![]),
            start: None,
            end: None,
            match_ranges: vec![],
            matched_lines: vec![],
            count: Some(0),
            warnings: vec![],
        });
    }
    let context = q
        .context_lines()
        .unwrap_or(if q.context_bytes().is_none() { 5 } else { 0 });
    let mut ranges: Vec<LineRange> = vec![];
    for &line in &hits {
        let r = LineRange {
            start: line.saturating_sub(context).max(1),
            end: (line + context).min(lines.len()),
        };
        if let Some(last) = ranges.last_mut().filter(|last| r.start <= last.end + 1) {
            last.end = last.end.max(r.end)
        } else {
            ranges.push(r)
        }
    }
    // `selected` maps each emitted view line to its source line; omission
    // markers between non-adjacent windows map to OMISSION_LINE.
    let mut selected = vec![];
    let mut text = String::new();
    let mut prev_end: Option<usize> = None;
    for r in &ranges {
        if let Some(prev) = prev_end.filter(|prev| r.start > prev + 1) {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n')
            }
            text.push_str(&omission_marker(prev + 1, r.start - 1));
            selected.push(OMISSION_LINE)
        }
        for line in r.start..=r.end {
            text.push_str(lines[line - 1]);
            selected.push(line)
        }
        prev_end = Some(r.end)
    }
    let long_lines = q.context_lines().is_none()
        && q.context_bytes().is_none()
        && hits
            .iter()
            .any(|line| lines[line - 1].len() > LONG_LINE_BYTES);
    let mut warnings = vec![];
    if long_lines {
        warnings.push(format!(
            "longMatchedLines: a matched line exceeds {LONG_LINE_BYTES} bytes (minified source), so each match is shown with {LONG_LINE_CONTEXT_BYTES} bytes of context; `... [N bytes omitted] ...` marks gaps. Set contextLines to read whole lines, or contextBytes to resize the windows."
        ));
    }
    if let Some(bytes) = q
        .context_bytes()
        .or(long_lines.then_some(LONG_LINE_CONTEXT_BYTES))
    {
        text = String::new();
        let mut last_end = 0;
        selected.clear();
        for (start, end) in spans {
            let mut a = start.saturating_sub(bytes);
            while !content.is_char_boundary(a) {
                a += 1
            }
            let mut b = (end + bytes).min(content.len());
            while b < content.len() && !content.is_char_boundary(b) {
                b += 1
            }
            // Overlapping windows continue from the previous end; re-emitting
            // the overlap would fabricate text that is not in the source.
            if !text.is_empty() {
                a = a.max(last_end)
            }
            if a >= b {
                continue;
            }
            if !text.is_empty() && a > last_end {
                if !text.ends_with('\n') {
                    text.push('\n')
                }
                text.push_str(&format!("... [{} bytes omitted] ...\n", a - last_end));
            }
            text.push_str(&content[a..b]);
            let first_line = content[..a].bytes().filter(|byte| *byte == b'\n').count() + 1;
            let last_byte = b.saturating_sub(1);
            let last_line = content.as_bytes()[..last_byte.min(content.len())]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                + 1;
            selected.extend(first_line..=last_line.max(first_line));
            last_end = b
        }
        selected.sort_unstable();
        selected.dedup();
        ranges = super::executor::compress_ranges(&selected);
    }
    Ok(Extraction {
        text,
        source_lines: Some(selected),
        start: ranges.first().map(|r| r.start),
        end: ranges.last().map(|r| r.end),
        match_ranges: ranges,
        matched_lines: hits.clone(),
        count: Some(hits.len()),
        warnings,
    })
}

/// Literal matches in original byte coordinates. Unicode lowercasing can
/// change byte length (e.g. İ → i + combining dot), so folded offsets must
/// never be used to slice the source. Only length-changing characters need
/// an entry in the offset map; ordinary text keeps an empty map.
fn literal_ranges(content: &str, pattern: &str, sensitive: bool) -> Vec<(usize, usize)> {
    if sensitive {
        return content
            .match_indices(pattern)
            .map(|(start, matched)| (start, start + matched.len()))
            .collect();
    }
    let folded = content.to_lowercase();
    let needle = pattern.to_lowercase();
    let mut changes = Vec::new();
    let mut folded_start = 0;
    for (start, character) in content.char_indices() {
        let source_end = start + character.len_utf8();
        let folded_end = folded_start + character.to_lowercase().map(char::len_utf8).sum::<usize>();
        if folded_end - folded_start != character.len_utf8() {
            changes.push((folded_start..folded_end, start..source_end));
        }
        folded_start = folded_end;
    }
    let original_offset = |offset, end| {
        let index = changes.partition_point(|(range, _)| range.end <= offset);
        if let Some((range, source)) = changes.get(index)
            && range.start < offset
        {
            return if end { source.end } else { source.start };
        }
        if index == 0 {
            offset
        } else {
            let (range, source) = &changes[index - 1];
            offset - range.end + source.end
        }
    };
    folded
        .match_indices(&needle)
        .map(|(start, matched)| {
            (
                original_offset(start, false),
                original_offset(start + matched.len(), true),
            )
        })
        .collect()
}
