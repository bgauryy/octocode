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
use crate::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadProgress, ReadProgressUnit};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::security::scan::ContentScan;

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
    share: Option<usize>,
    security: &S,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let path = local_fetch_query::path(q);
    let (resume, into_window) = resume_of(q);
    let kept = source
        .proven
        .and_then(|_| super::kept::hits(q, source.path, security));
    // A fresh scan's streamed judgment, when it proved and kept the source:
    // the page's windows are read from it, as from the scan's own proof.
    let mut judged_kept = None;
    let mut found = match kept {
        Some(hits) => hits.replay(q, resume),
        None => match super::changed_lines(source.path, source.file, source.meta, security, cancel)
            .map_err(|error| ExtractionError::new(ErrorCode::FileReadFailed, error))
            .and_then(|judged| {
                let (changed, judged_digest) = judged.unzip();
                let judged_digest = judged_digest.flatten();
                let redacted = changed.as_ref().map_or((0, 0), |changed| {
                    let values = changed
                        .values()
                        .map(|shown| shown.count(octocode_engine::security::REDACTION_PREFIX))
                        .sum();
                    (changed.len(), values)
                });
                let kept = judged_digest.as_deref().and_then(|digest| {
                    super::kept::of_digest(digest, source.path, source.meta, security)
                });
                let found = scan(
                    q,
                    super::scan::Judged {
                        file: source.file,
                        changed: changed.as_ref(),
                        kept: judged_digest.as_deref().filter(|_| kept.is_some()),
                    },
                    source.path,
                    resume,
                    source.scan,
                    cancel,
                )
                .map(|found| Scan { redacted, ..found })?;
                // The lines replaced are of the bytes the scan read.
                match (&judged_digest, &found.digest) {
                    (Some(judged), Some(read)) if judged != read => Err(ExtractionError::new(
                        ErrorCode::FileReadFailed,
                        "The source changed while it was read.".to_owned(),
                    )),
                    _ => Ok((found, kept)),
                }
            }) {
            Ok((found, kept)) => {
                judged_kept = kept;
                found
            }
            Err(error) => return prepare::extraction_error(q, error, 0),
        },
    };
    // Before any answer: a changed file with no hit past the cursor would
    // otherwise read as the old read's empty last page.
    let read = found
        .digest
        .as_deref()
        .or(found.proven.as_ref().map(|proven| proven.digest.as_str()));
    if let (Some(expected), Some(read)) = (local_fetch_query::snapshot(q), read)
        && expected != read
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
            ..LocalFetchResult::blank(ReadStatus::Empty)
        }
    } else {
        // A pass read every byte: the windows come from its checkpoints,
        // under the digest it proved.
        let proved = Opened {
            proven: found
                .proven
                .as_ref()
                .map(|proven| (proven, found.total_lines))
                .or(judged_kept
                    .as_deref()
                    .map(|kept| (&kept.proven, kept.total_lines)))
                .or(source.proven),
            ..*source
        };
        // A long hit line a line page cannot hold is walked alone, its
        // matches first: it starts its own page, as in a source held whole.
        let line_bytes = crate::source_view::pagination::line_page_bytes(share);
        let spans = super::walk::page_spans(&found, line_bytes);
        let walked = spans.first().and_then(|(_, hit)| *hit);
        let lines = spans
            .iter()
            .take_while(|(_, hit)| hit.is_none())
            .map(|(span, _)| *span)
            .collect::<Vec<_>>();
        let more = spans.len() > lines.len().max(1);
        let mut result = match walked.and_then(|index| found.long.get(index)) {
            Some(hit) => {
                let mut page = super::walk::walk_page(
                    q,
                    &proved,
                    &found,
                    (hit, into_window),
                    share,
                    security,
                    cancel,
                );
                // A read's first page states that it streams.
                if page.status != ReadStatus::Error && local_fetch_query::read_progress(q).is_none()
                {
                    page.warnings.insert(0, super::streamed_warning(source.len));
                }
                page
            }
            None => {
                let page = local_fetch_query::updated(
                    q,
                    serde_json::json!({"matchString":null,"regexEngine":null,"caseMode":null,"wholeWord":null,"contextLines":null,"block":null,"readProgress":null}),
                )
                .and_then(|mut page| {
                    local_fetch_query::set_line_spans(&mut page, &lines).map(|()| page)
                });
                match page {
                    Ok(mut page) => {
                        local_fetch_query::set_progress(
                            &mut page,
                            into_window.map(|bytes| ReadProgress {
                                unit: ReadProgressUnit::Bytes,
                                offset: wire_count(bytes),
                            }),
                        );
                        fetch_ranges(&page, &proved, share, security, cancel)
                    }
                    Err(error) => {
                        return LocalFetchResult::error(
                            path.to_owned(),
                            ErrorCode::ExecutionFailed,
                            error.to_string(),
                        );
                    }
                }
            }
        };
        if result.status == ReadStatus::Error {
            return result;
        }
        if let Err(error) = continue_matches(q, &mut result, &found, more) {
            return LocalFetchResult::error(path.to_owned(), ErrorCode::ExecutionFailed, error);
        }
        // A walk page's place says whether any page follows it.
        let continues = result
            .next
            .as_ref()
            .is_some_and(|next| next.continue_window.is_some());
        if let Some(page) = result.pagination.as_mut() {
            page.has_more = continues;
        }
        if continues {
            keep_scan(q, source, &mut found, judged_kept.is_some());
        }
        result
    };
    result.selected_match_count = Some(found.count);
    // A read's first page names the entries of a list that matched nothing.
    if found.count > 0 && found.limited.is_none() && local_fetch_query::read_progress(q).is_none() {
        let terms = local_fetch_query::match_strings(q);
        let unmatched = found
            .term_hits
            .iter()
            .zip(&terms)
            .filter(|(hit, _)| !**hit)
            .map(|(_, term)| (*term).to_owned())
            .collect::<Vec<_>>();
        result
            .warnings
            .extend(extraction::unmatched_warning(q, &unmatched));
    }
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
fn keep_scan(q: &LocalFetchQuery, source: &Opened<'_>, found: &mut Scan, judged_kept: bool) {
    // A scan whose streamed judgment proved and kept the source keeps only
    // its hits, under that judgment's digest.
    if judged_kept
        && !found.nul
        && let Some(digest) = found.digest.clone()
    {
        if let Some(hits) = KeptHits::of(found) {
            super::kept::keep_hits(q, source.path, &digest, hits, source.scan);
        }
        return;
    }
    let Some(proven) = found.proven.take().filter(|_| !found.nul) else {
        return;
    };
    let digest = proven.digest.clone();
    if super::kept::keep(source, proven, found.total_lines)
        && let Some(hits) = KeptHits::of(found)
    {
        super::kept::keep_hits(q, source.path, &digest, hits, source.scan);
    }
}

/// The page's hits, and the read of all that is left: from where the page
/// stopped, inside a window or past it, bound to this source version. `more`:
/// spans of the scan's windows lie past the page's.
fn continue_matches(
    q: &LocalFetchQuery,
    result: &mut LocalFetchResult,
    found: &Scan,
    more: bool,
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
    if cut.is_none() && !found.more && !more {
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
    next.continue_window = Some(FetchCall::page(rest));
    result.is_partial = Some(true);
    Ok(())
}
