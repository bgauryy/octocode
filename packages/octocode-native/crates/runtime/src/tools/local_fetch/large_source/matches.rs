//! `matchString` over a streamed source: one pass ([`super::scan`]) judges the
//! file in bounded windows with the whole-file matcher, counts every hit and
//! keeps the context windows of the next page; those windows are served as
//! streamed ranges. A later page resumes the scan after the last line an
//! earlier page served, so every hit is on exactly one page.

use super::ranges::fetch_ranges;
use super::scan::{Scan, scan};
use super::window::Opened;
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadProgress, ReadProgressUnit};

/// Serve the next page of `q`'s matches from the streamed `source`: the
/// context windows after the line its private progress names, as streamed
/// ranges, with every hit counted.
pub(super) fn fetch_matches(
    q: &LocalFetchQuery,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let path = local_fetch_query::path(q);
    let resume = local_fetch_query::offset(q).unwrap_or(0);
    let found = match scan(q, source.file, resume, cancel) {
        Ok(found) => found,
        Err(error) => return prepare::extraction_error(q, error, 0),
    };
    // Before any answer: a changed file with no hit past the cursor would
    // otherwise read as the old read's empty last page.
    if let (Some(expected), Some(proven)) = (local_fetch_query::snapshot(q), &found.proven)
        && expected != proven.digest
    {
        return stale_snapshot(q);
    }
    if found.binary {
        return super::super::acquire::refuse_binary(q, path);
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
            Ok(page) => {
                // The scan read every byte: the windows come from its
                // checkpoints, under the digest it proved.
                let proved = Opened {
                    proven: found
                        .proven
                        .as_ref()
                        .map(|proven| (proven, found.total_lines)),
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

/// The page's hits, and the read of the matches after it: from the end of
/// the window the page stopped in (that window reads on through its own
/// `nextPage`), bound to this source version.
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
    if !found.more {
        return Ok(());
    }
    let last = served.last().map_or(0, |range| range.end);
    let after = found
        .windows
        .iter()
        .find(|window| (window.0..=window.1).contains(&last))
        .map_or(last, |window| window.1);
    let mut rest = q.clone();
    local_fetch_query::set_progress(
        &mut rest,
        Some(ReadProgress {
            unit: ReadProgressUnit::Lines,
            offset: wire_count(after),
        }),
    );
    local_fetch_query::set_snapshot(&mut rest, result.source_sha256.as_deref())
        .map_err(|error| error.to_string())?;
    let rest = Some(FetchCall::continued(rest));
    let next = result.next.get_or_insert_with(NextCalls::default);
    if next.continue_window.is_some() {
        next.read_other_ranges = rest;
    } else {
        next.continue_window = rest;
    }
    result.is_partial = Some(true);
    Ok(())
}
