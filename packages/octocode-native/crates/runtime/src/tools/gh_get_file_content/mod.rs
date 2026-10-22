pub(crate) mod errors;
mod leads;

use crate::tools::local_fetch::types::local_fetch_query;
use octocode_contract_types::ErrorCode;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};

use crate::response::result::ToolData;
use crate::security::scan::{ContentScan, MemoizedScan};
use crate::tools::gh_shared::{GhFailure, locate_path, missing_path};
use crate::tools::local_fetch::{LocalFetchQuery, ReadStatus, process_fetched_content};
use leads::{empty_match_lead, last_window_repair, rewrite_continuations};
use octocode_github::{ContentRequest, GitHubProvider, ProviderError, RequestContext};

pub use octocode_contract_types::GhGetFileContentQuery;

/// One read's result. The wire row is flat: the read file's fields beside
/// `owner`/`repo` (a query reads one path).
#[derive(Clone, Debug, Serialize)]
pub struct GhGetFileContentResult {
    pub owner: String,
    pub repo: String,
    #[serde(flatten)]
    pub file: GhGetFileContentFile,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhGetFileContentFile {
    #[serde(flatten)]
    pub content: crate::tools::local_fetch::LocalFetchResult,
    /// Commit the read was pinned to (a branch or default-branch ref resolves
    /// to its current SHA; continuations reuse it).
    pub commit_sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_not_found: Option<bool>,
    #[serde(skip)]
    pub from_cache: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_by: Option<String>,
}

/// One ghGetFileContent row as the runtime emits it.
pub struct FileRead {
    pub output: ToolData,
    /// Every byte came from the content cache.
    pub cache: bool,
}

/// What shapes a fetched body, owned, so a ranked read shapes it on a
/// blocking worker: the scanner `S` (production's `ContentSecurity`) with
/// the shared sanitized-view memo, the call's context (its cancellation)
/// and the selection runtime a ranked read judges with.
#[derive(Clone)]
pub(crate) struct Shaping<S> {
    pub security: S,
    pub memo: Arc<crate::security::scan::SanitizedViewMemo>,
    pub context: crate::execution::ExecutionContext,
    pub selection: crate::tools::selection::Runtime,
}

/// Run one ghGetFileContent row: the read, or the failure with its one
/// recovery (a missing path walks to the case-corrected file or the nearest
/// existing directory).
pub(crate) async fn run<S: ContentScan + Clone + Send + Sync + 'static>(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    request: Result<&RequestContext, ProviderError>,
    window: Option<usize>,
    shaping: Shaping<S>,
) -> Result<FileRead, GhFailure> {
    let resolved = resolved_at(query).map_err(|error| errors::failure(error, query, None))?;
    let query = resolved.as_ref().unwrap_or(query);
    let request_context = request.map_err(|error| errors::failure(error, query, None))?;
    let error = match execute(provider, query, request_context, window, shaping).await {
        Ok(result) => {
            return file_read(result, query).map_err(|error| errors::failure(error, query, None));
        }
        Err(error) => error,
    };
    if error.reason == Some(octocode_github::ProviderErrorReason::Submodule) {
        let failure = errors::failure(error, query, None);
        return Err(crate::tools::gh_shared::submodule_failure(
            provider,
            &errors::repo_path(query),
            request_context,
            failure,
        )
        .await);
    }
    // A path failure comes after its ref resolved: its recovery leads pin
    // that commit, as the read's own continuations do, and name a renamed
    // repository as it is now.
    let scope = crate::tools::gh_shared::failure_scope(
        provider,
        query.owner.as_str(),
        query.repo.as_str(),
        query.ref_.as_ref().map(|value| value.as_str()),
        missing_path(&error, Some(query.path.as_str()))
            || error.reason == Some(octocode_github::ProviderErrorReason::PathIsDirectory),
        request_context,
    )
    .await;
    let pinned = scope.commit.and_then(|sha| {
        Some(GhGetFileContentQuery {
            ref_: Some(sha.parse().ok()?),
            ..query.clone()
        })
    });
    let query = pinned.as_ref().unwrap_or(query);
    let canonical = scope
        .canonical
        .and_then(|(owner, repo)| crate::tools::gh_shared::with_repo(query, &owner, &repo).ok());
    let query = canonical.as_ref().unwrap_or(query);
    let found = if missing_path(&error, Some(query.path.as_str())) {
        locate_path(provider, &errors::repo_path(query), request_context).await
    } else {
        None
    };
    let failure = errors::failure(error, query, found);
    Err(crate::tools::gh_shared::with_default_branch(
        provider,
        failure,
        crate::id::ToolId::GhGetFileContent,
        query,
        request_context,
    )
    .await)
}

/// The query with its `path` as the repository names it
/// ([`crate::tools::gh_shared::listing_path`], the rule every GitHub tool
/// reads a repository path by), so the read, its continuations and its
/// recovery leads name one path; `None` when it already does. A path that
/// climbs above the root, or names only the root, is invalid input, refused
/// before any request.
fn resolved_at(
    query: &GhGetFileContentQuery,
) -> Result<Option<GhGetFileContentQuery>, ProviderError> {
    let path = crate::tools::gh_shared::listing_path(query.path.as_str())?;
    if path.is_empty() {
        return Err(ProviderError::new(
            octocode_github::ProviderErrorKind::Validation,
            format!(
                "Path \"{}\" names the repository root, not a file.",
                query.path.as_str()
            ),
        ));
    }
    if path == query.path.as_str() {
        return Ok(None);
    }
    Ok(Some(GhGetFileContentQuery {
        path: path.parse().map_err(decode_display)?,
        ..query.clone()
    }))
}

/// The row of a read: its status from the file, and the resolved commit
/// unless the caller already named that full SHA.
fn file_read(
    result: GhGetFileContentResult,
    query: &GhGetFileContentQuery,
) -> Result<FileRead, ProviderError> {
    let status = result.file.content.status.row_status();
    let cache = result.file.from_cache;
    let mut data = serde_json::to_value(&result).map_err(|error| {
        ProviderError::new(
            octocode_github::ProviderErrorKind::Decode,
            error.to_string(),
        )
    })?;
    if data["commitSha"].as_str() == query.ref_.as_ref().map(|value| value.as_str())
        && let Some(map) = data.as_object_mut()
    {
        map.remove("commitSha");
    }
    Ok(FileRead {
        output: ToolData {
            status,
            ..ToolData::from(data)
        },
        cache,
    })
}

pub(crate) async fn execute<S: ContentScan + Clone + Send + Sync + 'static>(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    request_context: &RequestContext,
    window: Option<usize>,
    shaping: Shaping<S>,
) -> Result<GhGetFileContentResult, ProviderError> {
    // Resolve the ref once (memoized across a batch), then read the body at
    // the immutable SHA.
    let sha = provider
        .resolve_reference(
            &query.owner,
            &query.repo,
            query.ref_.as_ref().map(|value| value.as_str()),
            query.force_refresh.unwrap_or(false),
            request_context,
        )
        .await?;
    let content_request = ContentRequest {
        owner: query.owner.to_string(),
        repo: query.repo.to_string(),
        path: query.path.to_string(),
        reference: Some(sha),
        force_refresh: query.force_refresh.unwrap_or(false),
    };
    let acquired = provider
        .get_file_content(&content_request, request_context)
        .await?;
    // A renamed repository: later requests and every lead use the canonical
    // name (a redirect-free read of a never-redirected name costs nothing).
    let (renamed_warning, canonical) = crate::tools::gh_shared::canonical_query(
        provider,
        query,
        query.owner.as_str(),
        query.repo.as_str(),
        request_context,
    )
    .await?
    .unzip();
    let query = canonical.as_ref().unwrap_or(query);
    let fetched = Fetched {
        window,
        renamed_warning,
    };
    let mut result = if query.clasify_context.is_none() && query.selection.is_none() {
        let scanner = MemoizedScan::new(&shaping.security, &shaping.memo);
        shape(query, acquired, fetched, &scanner, &shaping.context, None)
    } else {
        // A ranked read judges its chunks: shaped on a blocking worker.
        let query = query.clone();
        tokio::task::spawn_blocking(move || {
            let scanner = MemoizedScan::new(&shaping.security, &shaping.memo);
            shape(
                &query,
                acquired,
                fetched,
                &scanner,
                &shaping.context,
                Some(&shaping.selection),
            )
        })
        .await
        .map_err(|_| {
            ProviderError::new(
                octocode_github::ProviderErrorKind::Decode,
                "ranked content worker failed",
            )
        })?
    }?;
    // A receipts read's first page states when the file last changed: one
    // commits request once the read succeeded, so a failed, throttled or
    // invalid read never spends it, and later pages repeat nothing. A
    // default read sends no extra request.
    if query
        .extras
        .contains(&octocode_contract_types::ExtraKind::Receipts)
        && query.offset.unwrap_or(0) == 0
        && result.file.content.error.is_none()
    {
        let (last_modified, last_modified_by) =
            file_timestamp(provider, query, &result.file.commit_sha, request_context).await;
        result.file.last_modified = last_modified;
        result.file.last_modified_by = last_modified_by;
    }
    Ok(result)
}

/// What the fetch side of a read hands its shaping: the response window
/// and the rename warning.
struct Fetched {
    window: Option<usize>,
    renamed_warning: Option<String>,
}

/// One fetched body as the read's row: the shared extraction, a missed
/// match's facts, the status, and the read's pages and leads.
fn shape(
    query: &GhGetFileContentQuery,
    acquired: octocode_github::ContentResponse,
    fetched: Fetched,
    security: &impl ContentScan,
    cancel: &crate::execution::ExecutionContext,
    selection: Option<&crate::tools::selection::Runtime>,
) -> Result<GhGetFileContentResult, ProviderError> {
    let Fetched {
        window,
        renamed_warning,
    } = fetched;
    let local = local_query(query)?;
    // Selected and whole-file views use the configured response budget;
    // bare reads retain the shared source-head preview policy. The shared
    // read fits the row's window less what this row adds around it.
    let facts = crate::tools::local_fetch::SourceFacts {
        modified: None,
        window,
        beside: row_envelope_chars(query, &acquired.resolved_ref),
        interior: false,
        byte_origin: None,
    };
    let mut content = if local.clasify_context.is_none() && local.selection.is_none() {
        process_fetched_content(
            &local,
            &acquired.bytes,
            Path::new(query.path.as_str()),
            &facts,
            security,
            cancel,
        )
    } else {
        let rank_path = format!(
            "{}/{}/{}/{}",
            query.owner, query.repo, acquired.resolved_ref, query.path
        );
        crate::tools::local_fetch::process_fetched_content_selected(
            &local,
            &acquired.bytes,
            Path::new(&rank_path),
            query.path.as_str(),
            &facts,
            security,
            cancel,
            selection,
        )
    };
    let match_not_found = settle_status(&mut content, query, &acquired.bytes)?;
    if let Some(warning) = renamed_warning {
        content.warnings.insert(0, warning);
    }
    let next = read_leads(&mut content, query, &local, match_not_found, &acquired);
    Ok(GhGetFileContentResult {
        owner: query.owner.to_string(),
        repo: query.repo.to_string(),
        file: GhGetFileContentFile {
            content,
            commit_sha: acquired.resolved_ref,
            file_type: file_type(&query.path),
            match_not_found: match_not_found.then_some(true),
            from_cache: acquired.from_cache,
            next,
            last_modified: None,
            last_modified_by: None,
        },
    })
}

/// Continuations a page's row may carry that the read rewrites with the
/// repository coordinates (its next window, the unfiltered view and the
/// next candidate page of a ranked read).
const REWRITTEN_CONTINUATIONS: usize = 3;
/// The quoted keys and separators of `"owner":"…","repo":"…","ref":"…"`,
/// the longer tool name and `forceRefresh` on one rewritten continuation.
const COORDINATE_KEYS_CHARS: usize = 64;
/// `commitSha` and `fileType` beside the row's content.
const ROW_FIELDS_CHARS: usize = 48;

/// Response chars a GitHub file row adds around the shared read's row: the
/// repository and resolved commit beside the content and on each
/// continuation the read rewrites. The shared read is cut to the row's
/// window less this, so the whole row fits that window.
fn row_envelope_chars(query: &GhGetFileContentQuery, sha: &str) -> usize {
    let coordinates =
        query.owner.as_str().len() + query.repo.as_str().len() + sha.len() + COORDINATE_KEYS_CHARS;
    coordinates * (1 + REWRITTEN_CONTINUATIONS) + ROW_FIELDS_CHARS
}

/// The read's status: a missed selection is an empty read that says why
/// ([`no_match`]), a read with text (or of a 0-byte file) a success,
/// anything else empty. Returns
/// whether the selection missed; an offset inside a code point fails.
fn settle_status(
    content: &mut crate::tools::local_fetch::LocalFetchResult,
    query: &GhGetFileContentQuery,
    bytes: &[u8],
) -> Result<bool, ProviderError> {
    if content.error_code == Some(ErrorCode::InvalidPagination) {
        return Err(ProviderError::new(
            octocode_github::ProviderErrorKind::Decode,
            "offset must be on a UTF-8 code point boundary",
        ));
    }
    let match_not_found = content.selected_match_count == Some(0);
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
        content.path = query.path.to_string();
    }
    Ok(match_not_found)
}

/// The read's leads: its own pages as GitHub reads at the read commit, a
/// missed match's search of the repository (unless the file itself can
/// still answer), and the last-window repair of ranges past the file end.
fn read_leads(
    content: &mut crate::tools::local_fetch::LocalFetchResult,
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
    match_not_found: bool,
    acquired: &octocode_github::ContentResponse,
) -> Option<Value> {
    let mut next = rewrite_continuations(
        content,
        query,
        local_fetch_query::minify_mode(local),
        &acquired.resolved_ref,
    );
    // A miss this file can still answer (hits outside the ranges, or
    // ignoring case) leads to that read, not back to a repository search.
    let answered_here = next
        .as_ref()
        .is_some_and(|calls| calls.get("read").is_some() || calls.get("ignoreCase").is_some());
    if match_not_found
        && !answered_here
        && let Some((name, lead)) = empty_match_lead(query, local)
        && let Some(calls) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        calls.insert(name.into(), lead);
    }
    if content.error.is_some()
        && let Some(lead) = last_window_repair(
            query,
            local,
            String::from_utf8_lossy(&acquired.bytes).lines().count(),
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
    content: &mut crate::tools::local_fetch::LocalFetchResult,
    query: &GhGetFileContentQuery,
    bytes: &[u8],
) {
    content.path = query.path.to_string();
    content.content = Some(String::new());
    content.content_view = None;
    content.total_lines = Some(String::from_utf8_lossy(bytes).lines().count());
    content.source_chars = None;
    content.source_bytes = None;
    content.returned_chars = None;
    content.returned_bytes = None;
    content.returned_lines = None;
    content.pagination = None;
    content.source_line_ranges.clear();
}

/// The kind of file a read returned (`fileType`).
fn file_type(path: &str) -> Option<&'static str> {
    match crate::content::classify_file_type(path)? {
        crate::content::FileType::Config => Some("config"),
        crate::content::FileType::Lock => Some("lock"),
        crate::content::FileType::Doc => Some("doc"),
        crate::content::FileType::Code => Some("code"),
    }
}

/// Last commit touching `path` at `reference` (a resolved SHA), from the one
/// source of that fact ([`octocode_github::GitHubTransport::path_last_commit`],
/// which ghStructure's dated listings share). History below a commit is
/// immutable, so the answer is cached per (owner, repo, SHA, path) and
/// repeated offset-0 reads skip the round trip.
async fn file_timestamp(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    reference: &str,
    context: &RequestContext,
) -> (Option<String>, Option<String>) {
    let partition = provider.transport.cache_partition(context, None).ok();
    let key = {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        for value in [
            query.owner.to_ascii_lowercase().as_str(),
            query.repo.to_ascii_lowercase().as_str(),
            reference,
            query.path.as_str(),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        // v2: the author is the login when GitHub links one.
        format!(
            "github-file-timestamp:v2-{}",
            hex::encode(digest.finalize())
        )
    };
    if let Some(partition) = &partition
        && let Some(cached) = provider.transport.cache.get(partition, &key).await
        && let Ok((date, author)) =
            serde_json::from_slice::<(Option<String>, Option<String>)>(&cached.bytes)
    {
        return (date, author);
    }
    let Ok(stamp) = provider
        .transport
        .path_last_commit(
            query.owner.as_str(),
            query.repo.as_str(),
            reference,
            query.path.as_str(),
            context,
        )
        .await
    else {
        return (None, None);
    };
    if let Some(partition) = &partition
        && let Ok(bytes) = serde_json::to_vec(&stamp)
    {
        provider
            .transport
            .cache
            .put(
                partition,
                key,
                octocode_github::CachedContent {
                    etag: None,
                    bytes: bytes.into(),
                    resolved_ref: reference.to_owned(),
                    has_more: false,
                },
                octocode_github::Retention::Pinned,
            )
            .await;
    }
    stamp
}

/// The GitHub file query is the localFetch extraction query plus repository
/// coordinates (`owner`, `repo`, `ref`, `forceRefresh`): the extraction
/// fields map one to one onto the localFetch wire type, so both tools share
/// one extraction request. Paged reads get the default page size.
fn local_query(query: &GhGetFileContentQuery) -> Result<LocalFetchQuery, ProviderError> {
    let GhGetFileContentQuery {
        block,
        case_mode,
        context_lines,
        extras,
        force_refresh: _,
        full_content,
        length,
        main_goal,
        match_string,
        minify,
        offset,
        owner: _,
        path,
        ranges,
        reasoning,
        clasify_context,
        selection,
        ref_: _,
        regex,
        repo: _,
        unit,
        whole_word,
    } = query;
    Ok(LocalFetchQuery {
        block: *block,
        case_mode: *case_mode,
        whole_word: *whole_word,
        context_lines: *context_lines,
        extras: extras.clone(),
        full_content: *full_content,
        length: *length,
        main_goal: main_goal
            .as_ref()
            .map(|goal| goal.as_str().parse())
            .transpose()
            .map_err(decode_display)?,
        match_string: match_string.clone(),
        minify: *minify,
        offset: *offset,
        path: path.as_str().parse().map_err(decode_display)?,
        ranges: ranges
            .iter()
            .map(|range| range.as_str().parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(decode_display)?,
        reasoning: reasoning
            .as_ref()
            .map(|reasoning| reasoning.as_str().parse())
            .transpose()
            .map_err(decode_display)?,
        regex: *regex,
        snapshot: None,
        clasify_context: clasify_context
            .as_ref()
            .map(|context| serde_json::to_value(context).and_then(serde_json::from_value))
            .transpose()
            .map_err(decode_display)?,
        selection: selection
            .as_ref()
            .map(|plan| plan.as_str().parse())
            .transpose()
            .map_err(decode_display)?,
        unit: *unit,
    })
}

fn decode_display(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::new(
        octocode_github::ProviderErrorKind::Decode,
        error.to_string(),
    )
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    fn fallback_hint(&self, query: &serde_json::Value) -> &'static str {
        if query.get("matchString").is_some() {
            "Match another literal, or remove matchString to read the file."
        } else {
            "Verify owner/repo/ref/path."
        }
    }
    fn error_hint(&self, code: ErrorCode) -> Option<&'static str> {
        // The message names the item; `repair` is the corrected read.
        (code == ErrorCode::InvalidPattern).then_some(crate::response::output::NO_HINT)
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
    fn text_shape(&self) -> crate::response::output::TextShape {
        crate::response::output::TextShape::FileText
    }
}

#[cfg(test)]
mod tests;
