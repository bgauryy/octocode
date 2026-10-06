use super::types::{LineRange, LocalFetchQuery, NextCalls, RegexMatch, line_read, uncovered};

pub struct Extraction {
    pub text: String,
    pub source_lines: Option<Vec<usize>>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub match_ranges: Vec<LineRange>,
    pub matched_lines: Vec<usize>,
    pub count: Option<usize>,
    pub warnings: Vec<String>,
    /// Continuations to the data this view selected but did not return.
    pub next: NextCalls,
    /// The declarations a `block:true` match read widened hits to.
    pub blocks: Vec<super::types::DeclaredBlock>,
}

fn follow_up(query: LocalFetchQuery, why: &str) -> Option<super::types::Continuation> {
    Some(super::types::Continuation {
        query,
        reason: Some(why.into()),
    })
}
/// A matched line longer than this (minified or bundled source) is read in
/// byte windows unless the caller chose a context.
const LONG_LINE_BYTES: usize = 2_000;
const LONG_LINE_CONTEXT_BYTES: usize = 200;
/// The rest of a declaration a match window cut is shown inline up to the
/// size of the `readBlock` lead it replaces (tool, path, line span).
const INLINE_REST_BYTES: usize = 200;
/// The `<line>\t` gutter each inlined line adds to the response.
const INLINE_GUTTER_BYTES: usize = 5;

/// View-line sentinel for an omission marker (source lines are 1-based).
pub const OMISSION_LINE: usize = 0;

/// Separates non-adjacent requested windows (ranges or match windows) so
/// readers never mistake a gap for contiguous source, nor for lines cut from
/// a span they asked for: a requested span is never elided.
pub fn omission_marker(start: usize, end: usize) -> String {
    if start == end {
        format!("... [line {start} not requested] ...\n")
    } else {
        format!("... [lines {start}-{end} not requested] ...\n")
    }
}
pub fn line_count(s: &str) -> usize {
    s.split_inclusive('\n').count()
}
pub fn extract(
    q: &LocalFetchQuery,
    content: &str,
    regex: &impl RegexMatch,
) -> Result<Extraction, String> {
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    let patterns = q.match_strings();
    if !patterns.is_empty() {
        return match_extract(q, content, &lines, &patterns, regex);
    }
    let requested = q.line_ranges();
    if !requested.is_empty() {
        return ranges_extract(q, content, &lines, &requested);
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
        next: NextCalls::default(),
        blocks: vec![],
    })
}
/// Line ranges joined into one view: sorted, overlapping or adjacent ranges
/// merged, clamped to the file end; with `block`, each widened to its
/// enclosing declaration.
fn ranges_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    requested: &[LineRange],
) -> Result<Extraction, String> {
    let total = lines.len();
    let mut warnings = vec![];
    let mut kept: Vec<LineRange> = vec![];
    for range in requested {
        if range.end < range.start {
            return Err(format!(
                "range {}-{} ends before it starts; use start-end with end >= start",
                range.start, range.end
            ));
        }
        if range.start > total {
            warnings.push(format!(
                "Range {}-{} starts past the file end ({total} lines); skipped.",
                range.start, range.end
            ));
            continue;
        }
        if range.end > total {
            warnings.push(format!(
                "Range {}-{} adjusted to end at {total} (file end).",
                range.start, range.end
            ));
        }
        kept.push(LineRange {
            start: range.start.max(1),
            end: range.end.min(total),
        });
    }
    if kept.is_empty() {
        return Err(format!(
            "Requested lines start past the file end ({total} lines)"
        ));
    }
    let mut rest = vec![];
    if q.block() {
        kept = super::block::widen_ranges(content, q.path(), kept, &mut warnings, &mut rest);
    }
    let windows = merge_ranges(kept);
    let (text, selected) = windows_text(lines, &windows);
    // A plain read that stops inside a declaration offers the first one it
    // cut. Lines between the read's own windows were skipped on purpose.
    let cut = if q.block() {
        Vec::new()
    } else {
        let edges: Vec<usize> = windows.iter().flat_map(|w| [w.start, w.end]).collect();
        super::block::enclosing(content, q.path(), &edges).unwrap_or_default()
    };
    let span = (
        windows.first().map_or(0, |w| w.start),
        windows.last().map_or(0, |w| w.end),
    );
    let read_block = cut
        .into_iter()
        .find(|block| block.start < span.0 || block.end > span.1)
        .and_then(|block| line_read(q, &block_lead(block, &windows, span)))
        .and_then(|query| follow_up(query, "Read the declaration this read cut."));
    let next = NextCalls {
        continue_block: line_read(q, &uncovered(&merge_ranges(rest), &windows)).and_then(|query| {
            follow_up(
                query,
                "Read the rest of the declaration the block window stopped inside.",
            )
        }),
        read_block,
        ..NextCalls::default()
    };
    Ok(Extraction {
        text,
        source_lines: Some(selected),
        start: windows.first().map(|r| r.start),
        end: windows.last().map(|r| r.end),
        match_ranges: vec![],
        matched_lines: vec![],
        count: None,
        warnings,
        next,
        blocks: vec![],
    })
}

/// The unseen lines of the first declaration in `blocks` (in hit order)
/// that `shown` does not cover whole; empty when every one is shown.
fn first_unseen(blocks: Vec<LineRange>, shown: &[LineRange]) -> Vec<LineRange> {
    blocks
        .into_iter()
        .map(|block| uncovered(&[block], shown))
        .find(|rest| !rest.is_empty())
        .unwrap_or_default()
}

/// The read that completes `block`, which a read spanning lines
/// `first..=last` and showing `shown` cuts: only its unseen lines when the
/// block extends past both ends of that span, else the whole declaration.
/// Either read spans the declaration, so it cuts nothing further: a walk of
/// these leads ends after one hop.
fn block_lead(
    block: LineRange,
    shown: &[LineRange],
    (first, last): (usize, usize),
) -> Vec<LineRange> {
    if block.start < first && block.end > last {
        uncovered(&[block], shown)
    } else {
        vec![block]
    }
}

/// Sorted ranges with overlapping or adjacent ones merged.
fn merge_ranges(ranges: Vec<LineRange>) -> Vec<LineRange> {
    crate::tools::line_spans::merge_spans(ranges.into_iter().map(|range| (range.start, range.end)))
        .into_iter()
        .map(|(start, end)| LineRange { start, end })
        .collect()
}

/// The text of `windows` (sorted, disjoint), with one omission marker
/// between non-adjacent windows; `selected` maps each view line to its
/// source line (markers map to [`OMISSION_LINE`]).
fn windows_text(lines: &[&str], windows: &[LineRange]) -> (String, Vec<usize>) {
    let mut selected = vec![];
    let mut text = String::new();
    let mut prev_end: Option<usize> = None;
    for r in windows {
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
    (text, selected)
}

/// Byte spans of every pattern (a list matches any of its entries), in
/// source order.
fn match_spans(
    q: &LocalFetchQuery,
    content: &str,
    patterns: &[&str],
    regex: &impl RegexMatch,
) -> Result<Vec<(usize, usize)>, String> {
    let mut spans = vec![];
    for pattern in patterns {
        let sensitive = q.case_sensitive_for(pattern);
        if q.is_pcre2() {
            spans.extend(pcre2_ranges(pattern, sensitive, content)?);
        } else if q.is_regex() {
            spans.extend(regex.matching_ranges(pattern, sensitive, content).map_err(|error| {
                if *pattern == "[" { "Invalid regex pattern: Invalid regular expression: /[/: Unterminated character class".to_owned() } else { error }
            })?);
        } else {
            spans.extend(literal_ranges(content, pattern, sensitive));
        }
    }
    if patterns.len() > 1 {
        spans.sort_unstable();
        spans.dedup();
    }
    Ok(spans)
}

/// Most matches one regex read records, as for `regex:"rust"`.
const MAX_REGEX_MATCHES: usize = 10_000;

/// `regex:"pcre2"` spans through the engine's deadline-bounded PCRE2 scan,
/// with the read's error prefixes (`invalidPattern`, `toolExecutionFailed`).
fn pcre2_ranges(
    pattern: &str,
    case_sensitive: bool,
    content: &str,
) -> Result<Vec<(usize, usize)>, String> {
    use octocode_engine::portable::{Pcre2RangesError, pcre2_find_ranges};
    pcre2_find_ranges(pattern, !case_sensitive, content, MAX_REGEX_MATCHES).map_err(|error| {
        match error {
            Pcre2RangesError::InvalidPattern(message) => {
                format!("Invalid regex pattern: {message}")
            }
            Pcre2RangesError::Unavailable(message) => {
                format!("Regex execution unavailable: {message}")
            }
        }
    })
}

/// 1-based lines any span touches: every line of a multiline match, not
/// just its start. A match ending at the next line's start does not touch it.
fn hit_lines(lines: &[&str], spans: &[(usize, usize)]) -> Vec<usize> {
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
        if spans
            .get(next_match)
            .is_some_and(|(start, _)| *start < line_end)
        {
            hits.push(i + 1)
        }
        line_start = line_end;
    }
    hits
}

/// A context window that stops inside its enclosing declaration shows the
/// declaration's unseen lines inline when they cost no more than a lead,
/// else leads to the rest of the top hit's declaration.
fn finish_cut_declarations(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    hits: &[usize],
    ranges: &mut Vec<LineRange>,
) -> Option<super::types::Continuation> {
    let blocks = super::block::enclosing(content, q.path(), hits)?;
    let rest = uncovered(&merge_ranges(blocks.clone()), ranges);
    let rest_bytes: usize = rest
        .iter()
        .flat_map(|range| &lines[range.start - 1..range.end])
        .map(|line| line.len() + INLINE_GUTTER_BYTES)
        .sum();
    if rest_bytes <= INLINE_REST_BYTES {
        *ranges = merge_ranges(std::mem::take(ranges).into_iter().chain(rest).collect());
        return None;
    }
    // Every cut declaration's read, in hit order: one lead covers them all.
    let reads: Vec<Vec<LineRange>> = blocks
        .into_iter()
        .filter_map(|block| {
            let inside = ranges
                .iter()
                .filter(|seen| seen.start <= block.end && seen.end >= block.start);
            let first = inside.clone().map(|seen| seen.start).min()?;
            let last = inside.map(|seen| seen.end).max()?;
            let cut = !uncovered(std::slice::from_ref(&block), ranges).is_empty();
            cut.then(|| block_lead(block, ranges, (first, last)))
        })
        .collect();
    let bytes = |read: &[LineRange]| -> usize {
        read.iter()
            .flat_map(|range| &lines[range.start - 1..range.end.min(lines.len())])
            .map(|line| line.len())
            .sum()
    };
    let all = merge_ranges(reads.iter().flatten().cloned().collect());
    // A lead past the cap (a minified bundle's IIFE) would page for many
    // calls: fall back to the top hit's declaration alone, or offer none.
    let read = if bytes(&all) <= READ_BLOCK_MAX_BYTES {
        all
    } else {
        reads
            .into_iter()
            .next()
            .filter(|top| bytes(top) <= READ_BLOCK_MAX_BYTES)?
    };
    let why = if read.len() > 1 {
        "Read the declarations the hit windows cut."
    } else {
        "Read the top hit's declaration."
    };
    line_read(q, &read).and_then(|query| follow_up(query, why))
}

/// Largest read a cut-declaration lead points at.
const READ_BLOCK_MAX_BYTES: usize = 32 * 1024;

/// Most bytes of whole lines a default match window (no `contextLines`, no
/// `contextBytes`) shows around each hit: long-line sources get fewer
/// lines, and `next.expandContext` shows the full default window.
const MATCH_WINDOW_BYTES: usize = 2 * 1024;

/// The default window around `hit` (1-based): up to `context` lines on
/// each side while the window's line bytes fit [`MATCH_WINDOW_BYTES`]; the
/// hit line is always whole. `true` when the window was narrowed.
fn byte_bounded_window(lines: &[&str], hit: usize, context: usize) -> (LineRange, bool) {
    let mut used = lines[hit - 1].len();
    let (mut start, mut end) = (hit, hit);
    let (mut before, mut after) = (true, true);
    for _ in 0..context {
        if before && start > 1 && used + lines[start - 2].len() <= MATCH_WINDOW_BYTES {
            start -= 1;
            used += lines[start - 1].len();
        } else {
            before = false;
        }
        if after && end < lines.len() && used + lines[end].len() <= MATCH_WINDOW_BYTES {
            end += 1;
            used += lines[end - 1].len();
        } else {
            after = false;
        }
    }
    let full = LineRange {
        start: hit.saturating_sub(context).max(1),
        end: (hit + context).min(lines.len()),
    };
    let narrowed = start > full.start || end < full.end;
    (LineRange { start, end }, narrowed)
}

/// The same read with the full default window around each hit.
fn expand_context_lead(q: &LocalFetchQuery) -> Option<super::types::Continuation> {
    let mut wide = q.clone();
    wide.context_lines = Some(DEFAULT_MATCH_CONTEXT_LINES as i64);
    wide.offset = None;
    wide.unit = None;
    wide.length = None;
    wide.snapshot = None;
    follow_up(wide, "Show the full ±10-line window around each hit.")
}

/// The whole matched lines a long-line match showed only byte windows of.
fn whole_lines_lead(q: &LocalFetchQuery) -> Option<super::types::Continuation> {
    let mut whole = q.clone();
    whole.context_lines = Some(0);
    whole.context_bytes = None;
    whole.offset = None;
    whole.unit = None;
    whole.length = None;
    whole.snapshot = None;
    follow_up(whole, "Read the whole matched lines, paged.")
}

/// Lines shown around each `matchString` hit when `contextLines` is unset
/// (and no `contextBytes` window replaces them).
const DEFAULT_MATCH_CONTEXT_LINES: usize = 10;

fn match_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    patterns: &[&str],
    regex: &impl RegexMatch,
) -> Result<Extraction, String> {
    let spans = match_spans(q, content, patterns, regex)?;
    let hits = hit_lines(lines, &spans);
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
            next: NextCalls::default(),
            blocks: vec![],
        });
    }
    let context = q.context_lines().unwrap_or(if q.context_bytes().is_none() {
        DEFAULT_MATCH_CONTEXT_LINES
    } else {
        0
    });
    let mut warnings = vec![];
    // An unset context sizes each window by bytes as well as lines, so a
    // long-line (minified) source does not return kilobytes per hit.
    let default_window = q.context_lines().is_none() && q.context_bytes().is_none();
    let mut narrowed = false;
    let windows: Vec<LineRange> = hits
        .iter()
        .map(|&line| {
            if default_window {
                let (window, cut) = byte_bounded_window(lines, line, context);
                narrowed |= cut;
                window
            } else {
                LineRange {
                    start: line.saturating_sub(context).max(1),
                    end: (line + context).min(lines.len()),
                }
            }
        })
        .collect();
    let mut oversized = vec![];
    let mut blocks = vec![];
    let windows = if q.block() && q.context_bytes().is_none() {
        super::block::widen_matches(
            content,
            q.path(),
            &hits,
            windows,
            &mut warnings,
            &mut oversized,
            &mut blocks,
        )
    } else {
        windows
    };
    let mut ranges = merge_ranges(windows);
    let mut next = NextCalls {
        read_block: line_read(q, &first_unseen(oversized, &ranges)).and_then(|query| {
            follow_up(
                query,
                "Read the top hit's declaration too large for a block window, minus the lines shown.",
            )
        }),
        ..NextCalls::default()
    };
    if !q.block()
        && q.context_bytes().is_none()
        && q.context_lines() != Some(0)
        && !hits
            .iter()
            .any(|line| lines[line - 1].len() > LONG_LINE_BYTES)
    {
        next.read_block = finish_cut_declarations(q, content, lines, &hits, &mut ranges);
    }
    // `selected` maps each emitted view line to its source line; omission
    // markers between non-adjacent windows map to OMISSION_LINE.
    let (mut text, mut selected) = windows_text(lines, &ranges);
    let long_lines = !q.block()
        && q.context_lines().is_none()
        && q.context_bytes().is_none()
        && hits
            .iter()
            .any(|line| lines[line - 1].len() > LONG_LINE_BYTES);
    if narrowed && !long_lines {
        let full = merge_ranges(
            hits.iter()
                .map(|&line| LineRange {
                    start: line.saturating_sub(context).max(1),
                    end: (line + context).min(lines.len()),
                })
                .collect(),
        );
        if !uncovered(&full, &ranges).is_empty() {
            warnings.push(format!(
                "Long lines: each hit shows whole lines within {MATCH_WINDOW_BYTES} bytes, fewer than ±{DEFAULT_MATCH_CONTEXT_LINES}; next.expandContext shows the full window."
            ));
            next.expand_context = expand_context_lead(q);
        }
    }
    if long_lines {
        warnings.push(format!(
            "longMatchedLines: a matched line exceeds {LONG_LINE_BYTES} bytes (minified source), so each match is shown with {LONG_LINE_CONTEXT_BYTES} bytes of context; `... [N bytes omitted] ...` marks gaps. next.wholeLines (contextLines:0) reads the whole matched lines; contextBytes resizes the windows."
        ));
        next.whole_lines = whole_lines_lead(q);
    }
    if let Some(bytes) = q
        .context_bytes()
        .or(long_lines.then_some(LONG_LINE_CONTEXT_BYTES))
    {
        (text, selected) = byte_windows(content, spans, bytes);
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
        next,
        blocks,
    })
}

/// Byte windows of `bytes` around each match span, with `... [N bytes
/// omitted] ...` between them (and, for long lines, before the first and
/// after the last): the text and the source line of every view line.
fn byte_windows(content: &str, spans: Vec<(usize, usize)>, bytes: usize) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut last_end = 0;
    let mut selected = Vec::new();
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
        } else if text.is_empty() {
            // The first window starting mid-line says what precedes it.
            let line_start = content[..a].rfind('\n').map_or(0, |at| at + 1);
            if a > line_start {
                text.push_str(&format!("... [{} bytes omitted] ...\n", a - line_start));
            }
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
    // The last window ending mid-line says what follows it.
    let line_end = content[last_end..]
        .find('\n')
        .map_or(content.len(), |at| last_end + at);
    if !text.is_empty() && line_end > last_end {
        if !text.ends_with('\n') {
            text.push('\n')
        }
        text.push_str(&format!(
            "... [{} bytes omitted] ...\n",
            line_end - last_end
        ));
    }
    selected.sort_unstable();
    selected.dedup();
    (text, selected)
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
