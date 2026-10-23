//! Several line ranges of a streamed source, held by one pass and joined.

use super::page::served_span;
use super::window::{Opened, serve_window, window_span};
use super::{read_bounded, streamed_warning, unsupported};
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use octocode_contract_types::ErrorCode;

/// Room an omission marker between two joined spans takes.
const OMISSION_MARKER_CHARS: usize = 48;

/// Several `ranges`: each (overlaps and neighbours merged, in line order) is
/// one streamed window read, joined with an omission marker between
/// non-adjacent spans, as a whole-file read joins them. The page ends at the
/// first span that does not fit one window page: it holds every span before
/// it and that span's first page, continued by that span's own pages
/// (`continueWindow`) and one read of every requested range after it
/// (`readOtherRanges`), so each requested line is on exactly one page.
pub(super) fn fetch_ranges(
    q: &LocalFetchQuery,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let len = source.len;
    let merged = crate::tools::line_spans::merge_spans(
        local_fetch_query::line_ranges(q)
            .into_iter()
            .map(|range| (range.start, range.end)),
    );
    let ones = merged
        .iter()
        .map(|&(start, end)| {
            let mut one = q.clone();
            local_fetch_query::set_line_span(&mut one, start, end);
            let span = window_span(&one);
            (one, span)
        })
        .collect::<Vec<_>>();
    // One pass over the file holds every range's window: the digest and
    // line count are the file's, not the range's.
    let wants = ones.iter().map(|(_, span)| span.want()).collect::<Vec<_>>();
    let streamed = match source.windows(&wants, cancel) {
        Ok(streamed) => streamed,
        Err(error) => {
            return LocalFetchResult::error(q.path.to_string(), ErrorCode::FileReadFailed, error);
        }
    };
    let mut parts = Vec::with_capacity(merged.len());
    // The span the page stops inside: its range and the calls that read on.
    let mut cut = None;
    // The row's share is spent by the first span's whole row and each later
    // span's text (with its omission marker); a later span is served with
    // the text room left plus the first row's other chars, which each part's
    // own row carries about as much of.
    let last_line = merged.last().map_or(0, |span| span.1);
    let mut text_left = share.map(|share| {
        share
            .saturating_sub(octocode_engine::json_size::string_body_units(&cut_note(
                last_line, last_line, true,
            )))
            .saturating_sub(4)
            .max(1)
    });
    let mut around = None;
    for (index, ((one, span), streamed)) in ones.into_iter().zip(streamed).enumerate() {
        let (start, end) = (span.first, span.requested_last);
        let part_share = text_left.map(|left| left + around.unwrap_or(0));
        let mut part = serve_window(
            &one,
            streamed,
            span,
            source,
            modified.clone(),
            part_share,
            security,
            cancel,
        );
        if part.status == ReadStatus::Error {
            return part;
        }
        if let Some(left) = text_left {
            let text = part.content.as_deref().map_or(0, |text| {
                crate::response::read_share::text_chars(text, part.total_lines.unwrap_or(0))
            });
            let row = serde_json::to_value(&part)
                .map_or(0, |row| crate::response::read_share::row_chars(&row));
            around.get_or_insert(row.saturating_sub(text));
            let spent = if index == 0 {
                row
            } else {
                text + OMISSION_MARKER_CHARS
            };
            text_left = Some(left.saturating_sub(spent).max(1));
        }
        if let Some(mut next) = part.next.take() {
            let after = &merged[index + 1..];
            next.read_other_ranges = (!after.is_empty()).then(|| {
                let mut rest = q.clone();
                local_fetch_query::set_line_spans(&mut rest, after);
                rest.snapshot = part.source_sha256.as_deref().and_then(|d| d.parse().ok());
                FetchCall { query: rest }
            });
            cut = Some((start, end, next));
            parts.push(part);
            break;
        }
        parts.push(part);
    }
    let mut parts = parts.into_iter();
    let Some(mut result) = parts.next() else {
        return unsupported(q, len, "no line range was requested.", read_bounded(q));
    };
    let mut spans_served = served_span(&result).into_iter().collect::<Vec<_>>();
    for mut part in parts {
        let span = served_span(&part);
        let (Some(content), Some(next_content)) = (result.content.as_mut(), part.content.take())
        else {
            continue;
        };
        let previous_end = spans_served.last().map_or(0, |span| span.1);
        if let Some((start, _)) = span
            && start > previous_end + 1
        {
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&omission_marker(previous_end + 1, start - 1));
        }
        content.push_str(&next_content);
        spans_served.extend(span);
        result.source_line_ranges.extend(part.source_line_ranges);
        for warning in part.warnings {
            if !result.warnings.contains(&warning) {
                result.warnings.push(warning);
            }
        }
        for hint in part.hints {
            if !result.hints.contains(&hint) {
                result.hints.push(hint);
            }
        }
        result.end_line = part.end_line.or(result.end_line);
        result.is_partial =
            (result.is_partial == Some(true) || part.is_partial == Some(true)).then_some(true);
        let add = |a: Option<usize>, b: Option<usize>| match (a, b) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
        result.returned_chars = add(result.returned_chars, part.returned_chars);
        result.returned_bytes = add(result.returned_bytes, part.returned_bytes);
        result.returned_lines = add(result.returned_lines, part.returned_lines);
    }
    // Each window held its lines; one line says it for all.
    result.warnings.insert(0, streamed_warning(len));
    result.pagination = None;
    if let Some((start, end, next)) = cut {
        result
            .warnings
            .push(cut_note(start, end, next.read_other_ranges.is_some()));
        result.is_partial = Some(true);
        result.next = Some(next);
    }
    result
}

/// The note of a ranges page that ends inside lines `start`-`end`, whose
/// later ranges (`after`) are read on their own.
fn cut_note(start: usize, end: usize, after: bool) -> String {
    let after = if after {
        "; the ranges after it are read on their own"
    } else {
        ""
    };
    format!(
        "Lines {start}-{end} need more than one window page: this page ends inside them and their later pages read that range alone{after}."
    )
}
