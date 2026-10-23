//! `matchString` reads: each hit's context window, grown to a small
//! declaration or narrowed to bytes around long lines.
use super::super::types::{
    ExtractionError, FetchCall, LineRange, LocalFetchQuery, NextCalls, RegexSpans, line_read,
    local_fetch_query, uncovered,
};
use super::byte_windows::byte_windows;
use super::search_lead::match_limit_lead;
use super::spans::{MAX_REGEX_MATCHES, hit_lines, match_spans};
use super::{Extraction, OMISSION_LINE, first_unseen, follow_up, merge_ranges, windows_text};
use octocode_contract_types::ErrorCode;

/// A matched line longer than this (minified or bundled source) is read in
/// byte windows unless the caller chose a context.
const LONG_LINE_BYTES: usize = 2_000;
const LONG_LINE_CONTEXT_BYTES: usize = 200;
/// The rest of a declaration a match window cut is shown inline up to this
/// size (about one lead's worth: tool, path, line span).
const INLINE_REST_BYTES: usize = 200;
/// Largest source whose declarations a match read parses to inline a cut
/// declaration's rest: the parse of a larger file costs far more than the
/// few lines it may add, so such a read keeps its plain context windows
/// (`block` or `ranges` reads the rest, as for a rest over
/// [`INLINE_REST_BYTES`]).
const INLINE_PARSE_MAX_BYTES: usize = 256 * 1024;
/// The `<line>\t` gutter each inlined line adds to the response.
const INLINE_GUTTER_BYTES: usize = 5;

/// A requested line range and the byte span it covers.
type Region = (LineRange, (usize, usize));

/// Each requested range clamped to the file, with its byte span in
/// `lines`' source: where a region grep counts hits.
fn region_bytes(lines: &[&str], ranges: &[LineRange]) -> Vec<Region> {
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut offset = 0;
    for line in lines {
        starts.push(offset);
        offset += line.len();
    }
    starts.push(offset);
    ranges
        .iter()
        .filter(|range| range.start <= lines.len())
        .map(|range| {
            let end = range.end.min(lines.len());
            (
                LineRange {
                    start: range.start,
                    end,
                },
                (starts[range.start - 1], starts[end]),
            )
        })
        .collect()
}

/// A match window that stops inside its declaration shows the rest inline
/// when that costs no more than [`INLINE_REST_BYTES`]; a larger rest stays
/// unshown, with no reread lead (`block` or `ranges` reads it).
fn inline_cut_declarations(
    content: &str,
    q: &LocalFetchQuery,
    lines: &[&str],
    hits: &[usize],
    ranges: &mut Vec<LineRange>,
) {
    let Some(blocks) = super::super::block::enclosing(content, local_fetch_query::path(q), hits)
    else {
        return;
    };
    let mut rest = uncovered(&merge_ranges(blocks), ranges);
    let requested = local_fetch_query::line_ranges(q);
    if !requested.is_empty() {
        rest = merge_ranges(
            rest.into_iter()
                .flat_map(|range| {
                    requested.iter().filter_map(move |requested| {
                        let start = range.start.max(requested.start);
                        let end = range.end.min(requested.end);
                        (start <= end).then_some(LineRange { start, end })
                    })
                })
                .collect(),
        );
    }
    let rest_bytes: usize = rest
        .iter()
        .flat_map(|range| &lines[range.start - 1..range.end])
        .map(|line| line.len() + INLINE_GUTTER_BYTES)
        .sum();
    if rest_bytes <= INLINE_REST_BYTES {
        *ranges = merge_ranges(std::mem::take(ranges).into_iter().chain(rest).collect());
    }
}

/// Most bytes of whole lines a default match window (no `contextLines`)
/// shows around each hit: long-line sources get fewer
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

/// The same read with `context_lines` around each hit and no page cursor:
/// the full default window (`expandContext`), or `0` for the whole matched
/// lines a long-line match showed only byte windows of (`wholeLines`).
fn context_lead(q: &LocalFetchQuery, context_lines: i64) -> Option<FetchCall> {
    let mut read = q.clone();
    read.context_lines = Some(context_lines);
    read.offset = None;
    read.unit = None;
    read.length = None;
    read.snapshot = None;
    follow_up(read)
}

/// Lines shown around each `matchString` hit when `contextLines` is unset:
/// contract `limits.defaultReadContextLines`.
const DEFAULT_MATCH_CONTEXT_LINES: usize = crate::id::DEFAULT_READ_CONTEXT_LINES;

/// The requested ranges' byte spans, with `spans` kept to the hits inside
/// them: with `ranges`, only those hits count, and their windows stay inside
/// the range that holds them. A range past the file end is invalid input.
fn ranged_spans(
    q: &LocalFetchQuery,
    lines: &[&str],
    spans: &mut Vec<(usize, usize)>,
) -> Result<Vec<Region>, ExtractionError> {
    let regions = region_bytes(lines, &local_fetch_query::line_ranges(q));
    if local_fetch_query::has_ranges(q) {
        if regions.is_empty() {
            return Err(ExtractionError::new(
                ErrorCode::InvalidInput,
                format!(
                    "Requested lines start past the file end ({} lines)",
                    lines.len()
                ),
            ));
        }
        spans.retain(|(start, end)| {
            regions
                .iter()
                .any(|(_, (from, to))| (*from..*to).contains(start) && end <= to)
        });
    }
    Ok(regions)
}

/// Each hit's context window, kept inside the requested range that holds
/// it, and whether a byte bound narrowed any: an unset context sizes each
/// window by bytes as well as lines, so a long-line (minified) source does
/// not return kilobytes per hit.
fn context_windows(
    q: &LocalFetchQuery,
    lines: &[&str],
    hits: &[usize],
    regions: &[Region],
) -> (Vec<LineRange>, bool) {
    let context = local_fetch_query::context_lines(q).unwrap_or(DEFAULT_MATCH_CONTEXT_LINES);
    let default_window = local_fetch_query::context_lines(q).is_none();
    let mut narrowed = false;
    let windows = hits
        .iter()
        .map(|&line| {
            let window = if default_window {
                let (window, cut) = byte_bounded_window(lines, line, context);
                narrowed |= cut;
                window
            } else {
                LineRange {
                    start: line.saturating_sub(context).max(1),
                    end: (line + context).min(lines.len()),
                }
            };
            let region = regions
                .iter()
                .map(|(range, _)| range)
                .find(|range| (range.start..=range.end).contains(&line));
            match region {
                Some(region) => LineRange {
                    start: window.start.max(region.start),
                    end: window.end.min(region.end),
                },
                None => window,
            }
        })
        .collect();
    (windows, narrowed)
}

pub(super) fn match_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &[&str],
    patterns: &[&str],
) -> Result<Extraction, ExtractionError> {
    let RegexSpans {
        mut spans,
        truncated,
    } = match_spans(q, content, patterns)?;
    let regions = ranged_spans(q, lines, &mut spans)?;
    let hits = hit_lines(lines, &spans);
    // The last line the capped scan reached: matches after it went unseen.
    let limit_line = truncated.then(|| hits.last().copied()).flatten();
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
            match_limited: false,
        });
    }
    let context = local_fetch_query::context_lines(q).unwrap_or(DEFAULT_MATCH_CONTEXT_LINES);
    let mut warnings = vec![];
    let (windows, narrowed) = context_windows(q, lines, &hits, &regions);
    let mut oversized = vec![];
    let mut blocks = vec![];
    let windows = if local_fetch_query::block(q) {
        super::super::block::widen_matches(
            content,
            local_fetch_query::path(q),
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
    // A requested declaration too large to show whole is unread evidence of
    // this read: its rest is a page.
    let mut next = NextCalls {
        continue_block: line_read(q, &first_unseen(oversized, &ranges)).and_then(follow_up),
        ..NextCalls::default()
    };
    // Only the default window may grow to a small declaration: an explicit
    // `contextLines` is that many lines, as `rg -C N` is.
    if !local_fetch_query::block(q)
        && local_fetch_query::context_lines(q).is_none()
        && content.len() <= INLINE_PARSE_MAX_BYTES
        && !hits
            .iter()
            .any(|line| lines[line - 1].len() > LONG_LINE_BYTES)
    {
        inline_cut_declarations(content, q, lines, &hits, &mut ranges);
    }
    // `selected` maps each emitted view line to its source line; omission
    // markers between non-adjacent windows map to OMISSION_LINE.
    let (mut text, mut selected) = windows_text(lines, &ranges);
    // `contextLines:0` asks for the matched lines themselves, whole; any
    // other context cannot show neighbouring lines of a minified line
    // without its kilobytes, so the hit's bytes are the window.
    let long_lines = !local_fetch_query::block(q)
        && local_fetch_query::context_lines(q) != Some(0)
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
                "Long lines: each hit shows whole lines within {MATCH_WINDOW_BYTES} bytes, fewer than ±{DEFAULT_MATCH_CONTEXT_LINES}."
            ));
            next.expand_context = context_lead(q, DEFAULT_MATCH_CONTEXT_LINES as i64);
        }
    }
    if long_lines {
        warnings.push(format!(
            "longMatchedLines: lines over {LONG_LINE_BYTES} bytes show {LONG_LINE_CONTEXT_BYTES} bytes around each match; `... [N bytes omitted] ...` marks gaps."
        ));
        next.whole_lines = context_lead(q, 0);
    }
    if let Some(line) = limit_line {
        warnings.push(format!(
            "Regex match limit: the scan stopped after {MAX_REGEX_MATCHES} matches at line {line}; matches after it are not selected; narrow the regex to select them."
        ));
        next.text_search = match_limit_lead(q, patterns);
    }
    if long_lines {
        let context_bytes = if regions.is_empty() {
            LONG_LINE_CONTEXT_BYTES
        } else {
            spans = spans
                .into_iter()
                .filter_map(|(start, end)| {
                    let (_, (from, to)) = regions
                        .iter()
                        .find(|(_, (from, to))| (*from..*to).contains(&start))?;
                    Some((
                        start.saturating_sub(LONG_LINE_CONTEXT_BYTES).max(*from),
                        end.saturating_add(LONG_LINE_CONTEXT_BYTES).min(*to),
                    ))
                })
                .collect();
            0
        };
        (text, selected) = byte_windows(content, spans, context_bytes);
        let mut source_lines = selected
            .iter()
            .copied()
            .filter(|line| *line != OMISSION_LINE)
            .collect::<Vec<_>>();
        source_lines.dedup();
        ranges = super::super::view::compress_ranges(&source_lines);
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
        match_limited: limit_line.is_some(),
    })
}
