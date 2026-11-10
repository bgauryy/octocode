//! Several line ranges of a streamed source, held by one pass and joined.

use super::page::served_span;
use super::window::{Opened, serve_window, window_span};
use super::{read_bounded, streamed_warning, unsupported};
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::extraction::omission_marker;
use crate::tools::source_view::local_fetch_query;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

/// Room an omission marker between two joined spans takes.
const OMISSION_MARKER_CHARS: usize = 48;

/// Several `ranges`: each (overlaps and neighbours merged, in line order) is
/// one streamed window read, joined with an omission marker between
/// non-adjacent spans, as a whole-file read joins them. The page ends at the
/// first span that does not fit one window page: it holds every span before
/// it and that span's first page. Its `nextPage` reads on from there through
/// every range after it, so following `nextPage` alone puts each requested
/// line on exactly one page. A read position belongs to the first span.
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
        .enumerate()
        .map(|(index, &(start, end))| {
            let mut one = q.clone();
            if index > 0 {
                local_fetch_query::set_progress(&mut one, None);
            }
            local_fetch_query::set_line_span(&mut one, start, end)?;
            let span = window_span(&one);
            Ok::<_, serde_json::Error>((one, span))
        })
        .collect::<Result<Vec<_>, _>>();
    let ones = match ones {
        Ok(ones) => ones,
        Err(error) => {
            return LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                ErrorCode::ExecutionFailed,
                error.to_string(),
            );
        }
    };
    // One pass over the file holds every range's window: the digest and
    // line count are the file's, not the range's. A proved source reads its
    // windows as the page reaches them, in growing runs: a page budget often
    // stops long before the last one.
    let wants = ones
        .iter()
        .map(|(_, span)| span.want(source.limits.window))
        .collect::<Vec<_>>();
    let mut held = std::collections::VecDeque::new();
    let (mut read, mut run) = (
        0,
        if source.proven.is_some() {
            1
        } else {
            wants.len()
        },
    );
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
            .saturating_sub(octocode_engine::json_size::string_body_units(
                &[(true, true), (false, true)]
                    .iter()
                    .filter_map(|&(continues, after)| {
                        cut_note(last_line, last_line, continues, after)
                    })
                    .max_by_key(String::len)
                    .unwrap_or_default(),
            ))
            .saturating_sub(4)
            .max(1)
    });
    let mut around = None;
    for (index, (one, span)) in ones.into_iter().enumerate() {
        if held.is_empty() {
            let upto = (read + run).min(wants.len());
            match source.windows(&wants[read..upto], cancel) {
                Ok(windows) => held.extend(windows),
                Err(error) => return super::window::read_failed(q, error),
            }
            (read, run) = (upto, run * 2);
        }
        let Some(streamed) = held.pop_front() else {
            break;
        };
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
            // A withheld byte page leaves no continuation of its own span.
            let continues = next.continue_window.is_some();
            if !after.is_empty()
                && let Err(error) = read_on(q, &mut next, after, part.source_sha256.as_deref())
            {
                return LocalFetchResult::error(
                    local_fetch_query::path(q).to_owned(),
                    ErrorCode::ExecutionFailed,
                    error,
                );
            }
            cut = Some((start, end, continues, !after.is_empty(), next));
            parts.push(part);
            break;
        }
        parts.push(part);
    }
    let mut parts = parts.into_iter();
    let Some(mut result) = parts.next() else {
        return unsupported(
            q,
            (len, source.limits),
            "no line range was requested.",
            read_bounded(q),
        );
    };
    join_parts(&mut result, parts);
    // Each window held its lines; one line says it for all, on the read's
    // first page.
    if local_fetch_query::read_progress(q).is_none() {
        result.warnings.insert(0, streamed_warning(len));
    }
    result.pagination = None;
    if let Some((start, end, continues, after, next)) = cut {
        if let Some(note) = cut_note(start, end, continues, after) {
            result.warnings.push(note);
        }
        result.is_partial = Some(true);
        result.next = Some(next);
    }
    result
}

/// Point `next.nextPage` of a page cut before the ranges `after` at the read of
/// all that is left: the cut span from where it stopped, then those ranges.
fn read_on(
    q: &LocalFetchQuery,
    next: &mut NextCalls,
    after: &[(usize, usize)],
    snapshot: Option<&str>,
) -> Result<(), String> {
    let (mut read, mut spans) = match next.continue_window.take() {
        Some(call) => {
            let cut = local_fetch_query::line_ranges(&call.query);
            let spans = cut.iter().map(|range| (range.start, range.end)).collect();
            (call.query, spans)
        }
        None => {
            let mut read = q.clone();
            local_fetch_query::set_progress(&mut read, None);
            (read, Vec::new())
        }
    };
    spans.extend_from_slice(after);
    local_fetch_query::set_line_spans(&mut read, &spans).map_err(|error| error.to_string())?;
    local_fetch_query::set_snapshot(&mut read, snapshot).map_err(|error| error.to_string())?;
    next.continue_window = Some(FetchCall::continued(read));
    Ok(())
}

fn join_parts(result: &mut LocalFetchResult, parts: impl Iterator<Item = LocalFetchResult>) {
    let mut spans_served = served_span(result).into_iter().collect::<Vec<_>>();
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
        // A part's terminal disclosure is the page's: it names what no page reads.
        if part.terminal_limit == Some(true) {
            result.terminal_limit = Some(true);
        }
        for reason in part.partial_reasons {
            if !result.partial_reasons.contains(&reason) {
                result.partial_reasons.push(reason);
            }
        }
        let add = |a: Option<usize>, b: Option<usize>| match (a, b) {
            (Some(a), Some(b)) => Some(a + b),
            (a, b) => a.or(b),
        };
        result.returned_bytes = add(result.returned_bytes, part.returned_bytes);
        result.returned_lines = add(result.returned_lines, part.returned_lines);
    }
}

/// The note of a ranges page that ends inside lines `start`-`end`: what next
/// reads, the rest of those lines when they `continue` and the requested ranges
/// `after` them. `None` when next reads neither: the page's own disclosure
/// names what is not read.
fn cut_note(start: usize, end: usize, continues: bool, after: bool) -> Option<String> {
    match (continues, after) {
        (true, true) => Some(format!(
            "Lines {start}-{end} need more than one window page: this page ends inside them and next reads on: first the rest of those lines, then the other ranges after them."
        )),
        (true, false) => Some(format!(
            "Lines {start}-{end} need more than one window page: this page ends inside them and their later pages read that range alone."
        )),
        (false, true) => Some(format!(
            "This page ends inside lines {start}-{end}, whose rest no page reads (see the warnings); next reads the other ranges after them."
        )),
        (false, false) => None,
    }
}

#[cfg(test)]
mod tests;
