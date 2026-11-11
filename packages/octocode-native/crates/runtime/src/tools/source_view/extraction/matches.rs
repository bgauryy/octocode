//! `matchString` reads: each hit's context window, grown to a small
//! declaration or narrowed to bytes around long lines.
use super::super::types::{ExtractionError, LineRange, NextCalls, RegexSpans, local_fetch_query};
use super::search_lead::match_limit_lead;
use super::spans::{hit_lines, match_spans, span_budget};
use super::{Extraction, LineOffsets, merge_ranges, placed_windows_text};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

/// A requested line range and the byte span it covers.
type Region = (LineRange, (usize, usize));

/// Each requested range clamped to the file, with its byte span in
/// `lines`' source: where a region grep counts hits.
fn region_bytes(lines: &LineOffsets, ranges: &[LineRange]) -> Vec<Region> {
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
                (lines.start(range.start - 1), lines.start(end)),
            )
        })
        .collect()
}

/// The requested ranges' byte spans, with `spans` kept to the hits inside
/// them: with `ranges`, only those hits count, and their windows stay inside
/// the range that holds them. A range past the file end is invalid input.
fn ranged_spans(
    q: &LocalFetchQuery,
    lines: &LineOffsets,
    spans: &mut Vec<(usize, usize)>,
) -> Result<Vec<Region>, ExtractionError> {
    let regions = region_bytes(lines, &local_fetch_query::line_ranges(q));
    if local_fetch_query::has_ranges(q) {
        if regions.is_empty() {
            return Err(ExtractionError::new(
                ErrorCode::InvalidInput,
                format!(
                    "Requested lines start past the file end ({})",
                    octocode_engine::text::counted(lines.len(), "line", "lines")
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

/// Requested context stays inside the selected source ranges.
fn context_windows(
    q: &LocalFetchQuery,
    lines: &LineOffsets,
    hits: &[usize],
    regions: &[Region],
) -> Vec<LineRange> {
    let context = local_fetch_query::context_lines(q);
    hits.iter()
        .map(|&line| {
            let window = LineRange {
                start: line
                    .saturating_sub(octocode_engine::num::saturating_usize(context.before))
                    .max(1),
                end: line
                    .saturating_add(octocode_engine::num::saturating_usize(context.after))
                    .min(lines.len()),
            };
            match regions
                .iter()
                .map(|(range, _)| range)
                .find(|range| (range.start..=range.end).contains(&line))
            {
                Some(region) => LineRange {
                    start: window.start.max(region.start),
                    end: window.end.min(region.end),
                },
                None => window,
            }
        })
        .collect()
}

pub(super) fn match_extract(
    q: &LocalFetchQuery,
    content: &str,
    lines: &LineOffsets,
    patterns: &[&str],
) -> Result<Extraction, ExtractionError> {
    let RegexSpans {
        mut spans,
        truncated,
        held: _spans_held,
    } = match_spans(q, content, patterns)?;
    let regions = ranged_spans(q, lines, &mut spans)?;
    let hits = hit_lines(lines, &spans);
    // The last line the capped scan reached: matches after it went unseen.
    let limit_line = truncated.then(|| hits.last().copied()).flatten();
    if hits.is_empty() {
        let mut next = NextCalls::default();
        let mut warnings = vec![];
        if truncated {
            warnings.push(match_limit_warning(None));
            next.text_search = match_limit_lead(q, patterns);
        }
        return Ok(Extraction {
            text: String::new(),
            source_lines: Some(vec![]),
            start: None,
            end: None,
            match_ranges: vec![],
            matched_lines: vec![],
            count: Some(0),
            warnings,
            next,
            blocks: vec![],
            match_limited: truncated,
            match_windows: vec![],
        });
    }
    let mut warnings = vec![];
    let windows = context_windows(q, lines, &hits, &regions);
    let mut blocks = vec![];
    let windows = if local_fetch_query::block(q) {
        super::super::block::widen_matches(
            content,
            local_fetch_query::path(q),
            &hits,
            windows,
            &mut warnings,
            &mut blocks,
        )
    } else {
        windows
    };
    let ranges = merge_ranges(windows);
    let mut next = NextCalls::default();
    let (text, selected, placed) = placed_windows_text(content, lines, &ranges);
    let match_windows = long_line_windows(content, lines, &spans, (&ranges, &placed));
    if let Some(line) = limit_line {
        warnings.push(match_limit_warning(Some(line)));
        next.text_search = match_limit_lead(q, patterns);
    }
    Ok(Extraction {
        text,
        source_lines: Some(selected),
        start: ranges.first().map(|r| r.start),
        end: ranges.last().map(|r| r.end),
        match_ranges: ranges,
        // Every match, however many share a line (a minified line holds many).
        count: Some(spans.len()),
        matched_lines: hits,
        warnings,
        next,
        blocks,
        match_limited: limit_line.is_some(),
        match_windows,
    })
}

/// Bytes shown on each side of a match in a line too long for a byte page:
/// the context of a hit whose line cannot be its context. A search row of
/// such a line shows the same.
pub(crate) const MATCH_MARGIN: usize = 80;

/// Bytes a line holds at most when every byte page holds it whole, whatever
/// its characters: the smallest page's room over the widest escape.
const FITS_ANY_BYTE_PAGE: usize = super::super::pagination::MIN_BYTE_PAGE / 6;

/// The bytes of the view around each match in a line longer than
/// [`FITS_ANY_BYTE_PAGE`]: [`MATCH_MARGIN`] on each side, inside the line,
/// every edge a clean cut, overlapping and touching windows merged. `shown`
/// is the view's line ranges with the view byte each starts at. The page
/// cutter decides which of these lines outgrow its byte page; their byte
/// walks show these windows first.
fn long_line_windows(
    content: &str,
    lines: &LineOffsets,
    spans: &[(usize, usize)],
    shown: (&[LineRange], &[usize]),
) -> Vec<(usize, usize)> {
    // Each window as asked, inside its line; spans come in source order, so
    // the windows of one line mostly merge as they arrive.
    let mut asked: Vec<(usize, usize)> = vec![];
    for &(start, end) in spans {
        let last = lines.line_at(end.max(start + 1) - 1);
        for line in lines.line_at(start)..=last {
            let (line_start, line_end) = (lines.start(line), lines.start(line + 1));
            if line_end - line_start <= FITS_ANY_BYTE_PAGE {
                continue;
            }
            let from = start.saturating_sub(MATCH_MARGIN).max(line_start);
            let to = end.saturating_add(MATCH_MARGIN).min(line_end);
            match asked.last_mut() {
                Some(before) if before.0 >= line_start && before.0 <= from && from <= before.1 => {
                    before.1 = before.1.max(to);
                }
                _ => asked.push((from, to)),
            }
        }
    }
    asked.sort_unstable();
    // Clean edges, in order: each scan for a marker looks back no further
    // than the clean cut before it, so a line is read once however many
    // windows it holds.
    let mut line = 0..0;
    let mut cut: Vec<(usize, usize)> = vec![];
    for (from, to) in asked {
        if !line.contains(&from) {
            let at = lines.line_at(from);
            line = lines.start(at)..lines.start(at + 1);
        }
        match cut.last_mut().filter(|before| before.0 >= line.start) {
            Some(before) if from <= before.1 => {
                before.1 =
                    crate::response::cut::clean_cut_past(content, before.1, to.max(before.1));
            }
            before => {
                let floor = before.map_or(line.start, |before| before.1);
                let start = crate::response::cut::clean_cut_from(content, floor, from);
                cut.push((
                    start,
                    crate::response::cut::clean_cut_past(content, start, to),
                ));
            }
        }
    }
    // Each window's place in the view: its line range's, plus its bytes
    // into that range.
    let (ranges, placed) = shown;
    cut.into_iter()
        .filter_map(|(start, end)| {
            let line = lines.line_at(start) + 1;
            let at = ranges.partition_point(|range| range.end < line);
            let range = ranges.get(at).filter(|range| range.start <= line)?;
            let view = placed.get(at)? + start - lines.start(range.start - 1);
            Some((view, view + end - start))
        })
        .collect()
}

/// The warning of a scan the memory budget stopped, after the last match it
/// held on `line` (none: before the first).
fn match_limit_warning(line: Option<usize>) -> String {
    let at = line.map_or_else(
        || "before its first match".to_owned(),
        |line| format!("at line {line}"),
    );
    format!(
        "Regex match limit: the scan stopped {at} because the process memory budget ({} bytes) has no room for more match spans; matches after it are not selected; narrow the regex to select them.",
        span_budget().limit()
    )
}
