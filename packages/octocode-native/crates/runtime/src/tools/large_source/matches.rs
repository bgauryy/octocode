//! `matchString` over a streamed source: one pass ([`super::scan`]) judges the
//! file in bounded windows with the whole-file matcher, counts every hit and
//! keeps the context windows of the next page; those windows are served as
//! streamed ranges. A settled source's later pages cut from that pass's hits
//! ([`super::kept`]) and read only their windows. A later page resumes after the last line an earlier page
//! served, inside a window when the page stopped there, so `nextPage` alone puts
//! every hit and context line on exactly one page.

use super::hits::KeptHits;
use super::ranges::fetch_ranges;
use super::scan::{Scan, scan};
use super::window::Opened;
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadProgress, ReadProgressUnit};

/// Bits of a later page's byte-unit progress that hold the bytes read of the
/// window it stopped in; the rest hold the line before that window. A match
/// read has one progress number and two positions to keep.
const RESUME_BYTE_BITS: u32 = 33;

/// Where `q` resumes: after this line, and these bytes into the next window.
fn resume_of(q: &LocalFetchQuery) -> (usize, Option<usize>) {
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    match local_fetch_query::unit(q) {
        ReadProgressUnit::Lines => (offset, None),
        ReadProgressUnit::Bytes => (
            offset >> RESUME_BYTE_BITS,
            Some(offset & ((1 << RESUME_BYTE_BITS) - 1)),
        ),
    }
}

/// The progress of a page that stopped `bytes` into the window after `line`.
fn byte_resume(line: usize, bytes: usize) -> Result<ReadProgress, String> {
    if bytes >> RESUME_BYTE_BITS != 0 || line >> (usize::BITS - 1 - RESUME_BYTE_BITS) != 0 {
        return Err(format!(
            "This read stopped {bytes} bytes into the window after line {line}, too far to resume; narrow it with ranges."
        ));
    }
    Ok(ReadProgress {
        unit: ReadProgressUnit::Bytes,
        offset: wire_count((line << RESUME_BYTE_BITS) | bytes),
    })
}

/// Serve the next page of `q`'s matches from the streamed `source`: the
/// context windows after the line its private progress names, as streamed
/// ranges, with every hit counted.
pub(super) fn fetch_matches<S: ContentScan>(
    q: &LocalFetchQuery,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &S,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let path = local_fetch_query::path(q);
    let (resume, into_window) = resume_of(q);
    let kept = source
        .proven
        .and_then(|_| super::kept::hits::<S>(q, source.path));
    let mut found = match kept {
        Some(hits) => hits.replay(q, resume),
        None => match scan(q, source.file, source.path, resume, source.scan, cancel) {
            Ok(found) => found,
            Err(error) => return prepare::extraction_error(q, error, 0),
        },
    };
    // Before any answer: a changed file with no hit past the cursor would
    // otherwise read as the old read's empty last page.
    if let (Some(expected), Some(proven)) = (local_fetch_query::snapshot(q), &found.proven)
        && expected != proven.digest
    {
        return stale_snapshot(q);
    }
    if found.binary {
        return refuse_binary(q, path);
    }
    let mut result = if found.windows.is_empty() {
        LocalFetchResult {
            path: path.to_owned(),
            content: Some(String::new()),
            total_lines: Some(found.total_lines),
            returned_bytes: Some(0),
            returned_lines: Some(0),
            modified,
            ..LocalFetchResult::blank(ReadStatus::Empty)
        }
    } else {
        let page = local_fetch_query::updated(
            q,
            serde_json::json!({"matchString":null,"regexEngine":null,"caseMode":null,"wholeWord":null,"contextLines":null,"block":null,"readProgress":null}),
        )
        .and_then(|mut page| {
            local_fetch_query::set_line_spans(&mut page, &found.windows).map(|()| page)
        });
        let mut result = match page {
            Ok(mut page) => {
                local_fetch_query::set_progress(
                    &mut page,
                    into_window.map(|bytes| ReadProgress {
                        unit: ReadProgressUnit::Bytes,
                        offset: wire_count(bytes),
                    }),
                );
                // A pass read every byte: the windows come from its
                // checkpoints, under the digest it proved.
                let proved = Opened {
                    proven: found
                        .proven
                        .as_ref()
                        .map(|proven| (proven, found.total_lines))
                        .or(source.proven),
                    ..*source
                };
                fetch_ranges(&page, &proved, modified, share, security, cancel)
            }
            Err(error) => {
                return LocalFetchResult::error(
                    path.to_owned(),
                    ErrorCode::ExecutionFailed,
                    error.to_string(),
                );
            }
        };
        if result.status == ReadStatus::Error {
            return result;
        }
        if let Err(error) = continue_matches(q, &mut result, &found) {
            return LocalFetchResult::error(path.to_owned(), ErrorCode::ExecutionFailed, error);
        }
        if result
            .next
            .as_ref()
            .is_some_and(|next| next.continue_window.is_some())
        {
            keep_scan::<S>(q, source, &mut found);
        }
        result
    };
    result.selected_match_count = Some(found.count);
    if let Some(warning) = super::edges::cut_warning(q, &found) {
        result.warnings.push(warning);
    }
    if let Some(line) = found.limited {
        result.warnings.push(format!(
            "Regex match limit: the scan stopped at line {line}; matches after it are not selected; narrow the regex to select them."
        ));
        result
            .next
            .get_or_insert_with(NextCalls::default)
            .text_search = extraction::search_lead(q, path, &local_fetch_query::match_strings(q));
    }
    result
}

/// Keep a fresh scan for the later pages of its read.
fn keep_scan<S: ContentScan>(q: &LocalFetchQuery, source: &Opened<'_>, found: &mut Scan) {
    let Some(proven) = found.proven.take().filter(|_| !found.nul) else {
        return;
    };
    let digest = proven.digest.clone();
    if super::kept::keep(source, proven, found.total_lines)
        && let Some(hits) = KeptHits::of(found)
    {
        super::kept::keep_hits::<S>(q, source.path, &digest, hits);
    }
}

/// The page's hits, and the read of all that is left: from where the page
/// stopped, inside a window or past it, bound to this source version.
fn continue_matches(
    q: &LocalFetchQuery,
    result: &mut LocalFetchResult,
    found: &Scan,
) -> Result<(), String> {
    let served = result.source_line_ranges.clone();
    result.matched_lines = found
        .hits
        .iter()
        .copied()
        .filter(|line| {
            served
                .iter()
                .any(|range| (range.start..=range.end).contains(line))
        })
        .collect();
    result.match_ranges.clone_from(&served);
    let cut = result
        .next
        .as_mut()
        .and_then(|next| next.continue_window.take());
    if cut.is_none() && !found.more {
        return Ok(());
    }
    let lines = |line: usize| ReadProgress {
        unit: ReadProgressUnit::Lines,
        offset: wire_count(line),
    };
    let progress = match &cut {
        None => lines(served.last().map_or(0, |range| range.end)),
        Some(call) => {
            let first = local_fetch_query::line_ranges(&call.query)
                .first()
                .map_or(1, |range| range.start);
            let read = local_fetch_query::offset(&call.query).unwrap_or(0);
            match local_fetch_query::unit(&call.query) {
                ReadProgressUnit::Lines => lines(first + read - 1),
                ReadProgressUnit::Bytes => byte_resume(first - 1, read)?,
            }
        }
    };
    let mut rest = q.clone();
    local_fetch_query::set_progress(&mut rest, Some(progress));
    local_fetch_query::set_snapshot(&mut rest, result.source_sha256.as_deref())
        .map_err(|error| error.to_string())?;
    let next = result.next.get_or_insert_with(NextCalls::default);
    next.continue_window = Some(FetchCall::continued(rest));
    result.is_partial = Some(true);
    Ok(())
}
