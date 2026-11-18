//! A fetched body as the read's row: the shared extraction, status, missed-match facts and leads.
use super::access;
use super::leads::{empty_match_leads, last_window_repair, rewrite_continuations};
use super::{GhGetFileContentFile, GhGetFileContentQuery, GhGetFileContentResult};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_github::ProviderError;
use octocode_runtime_base::security::scan::ContentScan;
use octocode_tools_shared::source_view::local_query::local_query;
use octocode_tools_shared::source_view::{ReadStatus, process_fetched_content};
use serde_json::{Value, json};
use std::path::Path;

/// What the fetch side of a read hands its shaping: the response window
/// and the rename warning.
pub(super) struct Fetched {
    pub(super) window: Option<usize>,
    pub(super) renamed_warning: Option<String>,
    /// The path a symlink at the read path resolved to: the file whose
    /// bytes these are, so its name picks the scan rules.
    pub(super) symlink_target: Option<String>,
}

/// One fetched body as the read's row: the shared extraction, a missed
/// match's facts, the status, and the read's pages and leads.
pub(super) fn shape(
    query: &GhGetFileContentQuery,
    acquired: octocode_github::ContentResponse,
    fetched: Fetched,
    security: &impl ContentScan,
    cancel: &octocode_runtime_base::execution::ExecutionContext,
    selection: Option<&octocode_tools_shared::selection::Runtime>,
) -> Result<GhGetFileContentResult, ProviderError> {
    let Fetched {
        window,
        renamed_warning,
        symlink_target,
    } = fetched;
    let scan_name = symlink_target.as_deref().unwrap_or(access::path(query));
    let local = local_query(query)?;
    // Selected and whole-file views use the configured response budget;
    // bare reads retain the shared source-head preview policy. The shared
    // read fits the row's window less what this row adds around it.
    let facts = octocode_tools_shared::source_view::SourceFacts {
        window,
        beside: row_envelope_chars(query, &acquired.resolved_ref),
        interior: false,
        byte_origin: None,
        file_lines: None,
        local_file: false,
    };
    let mut content =
        if !octocode_tools_shared::source_view::local_fetch_query::has_classification(&local)
            && octocode_tools_shared::source_view::local_fetch_query::selection(&local).is_none()
        {
            process_fetched_content(
                &local,
                &acquired.bytes,
                Path::new(scan_name),
                &facts,
                security,
                cancel,
            )
        } else {
            let rank_path = format!(
                "{}/{}/{}/{}",
                access::owner(query),
                access::repo(query),
                acquired.resolved_ref,
                scan_name
            );
            octocode_tools_shared::source_view::process_fetched_content_selected(
                &local,
                &acquired.bytes,
                Path::new(&rank_path),
                access::path(query),
                &facts,
                security,
                cancel,
                selection,
            )
        };
    let match_not_found = settle_status(&mut content, query, &acquired.bytes)?;
    if let Some(pointer) = super::lfs::parse(&acquired.bytes) {
        content.warnings.push(super::lfs::warning(
            &pointer,
            access::owner(query),
            access::repo(query),
            &acquired.resolved_ref,
            access::path(query),
        ));
    }
    if let Some(warning) = renamed_warning {
        content.warnings.insert(0, warning);
    }
    let next = read_leads(&mut content, query, &local, match_not_found, &acquired);
    Ok(GhGetFileContentResult {
        owner: access::owner(query).to_owned(),
        repo: access::repo(query).to_owned(),
        file: GhGetFileContentFile {
            content,
            commit_sha: acquired.resolved_ref,
            symlink_target,
            match_not_found: match_not_found.then_some(true),
            calls: next,
        },
    })
}

/// Continuations a page's row may carry that the read rewrites with the
/// repository coordinates (its next window, the unfiltered view and the
/// next candidate page of a ranked read).
const REWRITTEN_CONTINUATIONS: usize = 3;
/// The quoted keys and separators of `"owner":"…","repo":"…","ref":"…"`,
/// and the longer tool name on one rewritten continuation.
const COORDINATE_KEYS_CHARS: usize = 64;
/// `commitSha` beside the row's content.
const ROW_FIELDS_CHARS: usize = 48;

/// Response chars a GitHub file row adds around the shared read's row: the
/// repository and resolved commit beside the content and on each
/// continuation the read rewrites. The shared read is cut to the row's
/// window less this, so the whole row fits that window.
pub(super) fn row_envelope_chars(query: &GhGetFileContentQuery, sha: &str) -> usize {
    let coordinates =
        access::owner(query).len() + access::repo(query).len() + sha.len() + COORDINATE_KEYS_CHARS;
    coordinates * (1 + REWRITTEN_CONTINUATIONS) + ROW_FIELDS_CHARS
}

/// The read's status: a missed selection is an empty read that says why
/// ([`no_match`]), a read with text (or of a 0-byte file) a success,
/// anything else empty. Returns
/// whether the selection missed; an offset inside a code point fails.
fn settle_status(
    content: &mut octocode_tools_shared::source_view::LocalFetchResult,
    query: &GhGetFileContentQuery,
    bytes: &[u8],
) -> Result<bool, ProviderError> {
    if content.error_code == Some(ErrorCode::InvalidPagination) {
        return Err(ProviderError::new(
            octocode_github::ProviderErrorKind::Decode,
            "offset must be on a UTF-8 code point boundary",
        ));
    }
    // A classifier that kept nothing is a judgment, not a term that missed.
    let match_not_found = content.selected_match_count == Some(0) && content.selection.is_none();
    if match_not_found {
        no_match(content, query, bytes);
    }
    if content.returned_bytes == Some(0) {
        content.source_line_ranges.clear();
    }
    if content.error.is_none() {
        content.status = if match_not_found {
            // Same as localFetch: no selected line is an empty read, which
            // keeps the recovery hint visible under the hint policy.
            ReadStatus::Empty
        } else if content.content.as_ref().is_some_and(|s| !s.is_empty()) || bytes.is_empty() {
            // A 0-byte file read whole is its whole content, not a miss.
            ReadStatus::Success
        } else {
            ReadStatus::Empty
        };
    }
    // Shared local read errors omit their path; the remote file envelope
    // requires the repository-relative identity even when processing fails.
    if content.path.is_empty() {
        content.path = access::path(query).to_owned();
    }
    Ok(match_not_found)
}

/// The read's leads: its own pages as GitHub reads at the read commit, a
/// missed match's search of the repository (unless the file itself can
/// still answer), and the last-window repair of ranges past the file end.
fn read_leads(
    content: &mut octocode_tools_shared::source_view::LocalFetchResult,
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
    match_not_found: bool,
    acquired: &octocode_github::ContentResponse,
) -> Option<Value> {
    let mut next = rewrite_continuations(content, query, &acquired.resolved_ref);
    // A miss this file can still answer (hits outside the ranges, or
    // ignoring case) leads to that read, not back to a repository search.
    let answered_here = next
        .as_ref()
        .is_some_and(|calls| calls.get("read").is_some() || calls.get("ignoreCase").is_some());
    let misses = if match_not_found && !answered_here {
        empty_match_leads(query, local)
    } else {
        Vec::new()
    };
    if !misses.is_empty()
        && let Some(calls) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        for (name, lead) in misses {
            calls.insert(name.into(), lead);
        }
    }
    if content.error.is_some()
        && let Some(lead) = last_window_repair(
            query,
            local,
            octocode_engine::text::decode(&acquired.bytes)
                .text
                .lines()
                .count(),
            &acquired.resolved_ref,
        )
        && let Some(calls) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        calls.insert("repair".into(), lead);
    }
    next
}

/// A selection with no hit in the file is an empty read that says why: the
/// shared read's facts (redaction, a literal `|`, the lines a
/// case-insensitive read or a read without `ranges` would select) and the
/// file's size, with the contract's empty `content` (as in localFetch) and
/// no zero counters.
fn no_match(
    content: &mut octocode_tools_shared::source_view::LocalFetchResult,
    query: &GhGetFileContentQuery,
    bytes: &[u8],
) {
    content.path = access::path(query).to_owned();
    content.content = Some(String::new());

    content.total_lines = Some(octocode_engine::text::decode(bytes).text.lines().count());
    content.returned_bytes = None;
    content.returned_lines = None;
    content.pagination = None;
    content.source_line_ranges.clear();
}
