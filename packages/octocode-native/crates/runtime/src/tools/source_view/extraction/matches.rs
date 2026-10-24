//! `matchString` reads: each hit's context window, grown to a small
//! declaration or narrowed to bytes around long lines.
use super::super::types::{
    ExtractionError, LineRange, NextCalls, RegexSpans, line_read, local_fetch_query,
};
use super::search_lead::match_limit_lead;
use super::spans::{MAX_REGEX_MATCHES, hit_lines, match_spans};
use super::{Extraction, first_unseen, follow_up, merge_ranges, windows_text};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

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

/// Requested context stays inside the selected source ranges.
fn context_windows(
    q: &LocalFetchQuery,
    lines: &[&str],
    hits: &[usize],
    regions: &[Region],
) -> Vec<LineRange> {
    let context = local_fetch_query::context_lines(q);
    hits.iter()
        .map(|&line| {
            let window = LineRange {
                start: line
                    .saturating_sub(crate::num::usize_of(context.before))
                    .max(1),
                end: line
                    .saturating_add(crate::num::usize_of(context.after))
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
    let mut warnings = vec![];
    let windows = context_windows(q, lines, &hits, &regions);
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
    let ranges = merge_ranges(windows);
    // A requested declaration too large to show whole is unread evidence of
    // this read: its rest is a page.
    let mut next = NextCalls {
        continue_block: line_read(q, &first_unseen(oversized, &ranges)).and_then(follow_up),
        ..NextCalls::default()
    };
    let (text, selected) = windows_text(lines, &ranges);
    if let Some(line) = limit_line {
        warnings.push(format!(
            "Regex match limit: the scan stopped after {MAX_REGEX_MATCHES} matches at line {line}; matches after it are not selected; narrow the regex to select them."
        ));
        next.text_search = match_limit_lead(q, patterns);
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
