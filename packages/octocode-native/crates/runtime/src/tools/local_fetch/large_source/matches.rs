//! `matchString` over a streamed source: one pass judges the file in
//! line-aligned chunks with the whole-file matcher, counts every hit and
//! keeps the context windows of the next page; those windows are served as
//! streamed ranges. A later page resumes the scan after the last line an
//! earlier page served, so every hit is on exactly one page.

use super::ranges::fetch_ranges;
use super::window::Opened;
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadProgress, ReadProgressUnit};
use std::io::BufRead;

/// Source bytes the matcher judges at once; a chunk ends at a line end, so
/// one longer line is judged whole. Sizes the work, never the result.
const SCAN_CHUNK_BYTES: usize = 1024 * 1024;

/// What one scan found: every hit's count, the next page's merged windows
/// and hit lines, and whether windows remain past them.
#[derive(Default)]
struct Scan {
    count: usize,
    windows: Vec<(usize, usize)>,
    hits: Vec<usize>,
    more: bool,
    total_lines: usize,
    binary: bool,
    /// The line a regex scan stopped at its match limit.
    limited: Option<usize>,
}

impl Scan {
    /// Count hit `line` and keep its context `window` when the window reaches
    /// past `resume`, the last line earlier pages served.
    fn hit(&mut self, line: usize, window: (usize, usize), resume: usize) {
        self.count += 1;
        if window.1 <= resume {
            return;
        }
        let window = (window.0.max(resume + 1), window.1);
        let room = !self.more && self.windows.len() < RANGES_MAX_ITEMS;
        match self.windows.last_mut() {
            Some(last) if window.0 <= last.1 + 1 => {
                last.1 = last.1.max(window.1);
                self.hits.push(line);
            }
            _ if room => {
                self.windows.push(window);
                self.hits.push(line);
            }
            _ => self.more = true,
        }
    }
}

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
            Ok(page) => fetch_ranges(&page, source, modified, share, security, cancel),
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

/// Judge the whole source behind `file` for `q`'s patterns, from its start.
fn scan(
    q: &LocalFetchQuery,
    file: &std::fs::File,
    resume: usize,
    cancel: &impl CancellationCheck,
) -> Result<Scan, ExtractionError> {
    let failed = |error: String| ExtractionError::new(ErrorCode::FileReadFailed, error);
    let context = local_fetch_query::context_lines(q);
    let (before, after) = (
        crate::num::usize_of(context.before),
        crate::num::usize_of(context.after),
    );
    let regions = local_fetch_query::line_ranges(q);
    let ranged = local_fetch_query::has_ranges(q);
    let mut reader = std::io::BufReader::new(file);
    std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(0))
        .map_err(|error| failed(error.to_string()))?;
    let mut found = Scan::default();
    let (mut chunk, mut base, mut lines) = (Vec::new(), 0usize, 0usize);
    loop {
        let read = reader
            .read_until(b'\n', &mut chunk)
            .map_err(|error| failed(error.to_string()))?;
        lines += usize::from(read > 0);
        if read > 0 && chunk.len() < SCAN_CHUNK_BYTES {
            continue;
        }
        if !chunk.is_empty() && found.limited.is_none() {
            cancel.check().map_err(failed)?;
            found.binary |= chunk.contains(&0);
            let text = String::from_utf8_lossy(&chunk);
            let (hits, truncated) = extraction::match_lines(q, &text)?;
            for line in hits.iter().map(|line| base + line) {
                let mut window = (
                    line.saturating_sub(before).max(1),
                    line.saturating_add(after),
                );
                if ranged {
                    let Some(region) = regions
                        .iter()
                        .find(|range| (range.start..=range.end).contains(&line))
                    else {
                        continue;
                    };
                    window = (window.0.max(region.start), window.1.min(region.end));
                }
                found.hit(line, window, resume);
            }
            if truncated {
                found.limited = Some(hits.last().map_or(base, |line| base + line));
            }
        }
        base = lines;
        chunk.clear();
        if read == 0 {
            break;
        }
    }
    found.total_lines = lines;
    for window in &mut found.windows {
        window.1 = window.1.min(lines);
    }
    Ok(found)
}
