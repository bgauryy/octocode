pub(crate) mod errors;
mod leads;
mod local_query;
mod shape;
mod timestamp;

use octocode_contract_types::ErrorCode;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::response::result::ToolData;
use crate::security::scan::{ContentScan, MemoizedScan};
use crate::tools::gh_shared::{GhFailure, locate_path, missing_path};
use local_query::decode_display;
use octocode_github::{ContentRequest, GitHubProvider, ProviderError, RequestContext};
use shape::{Fetched, shape};
use timestamp::file_timestamp;

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
    pub content: crate::tools::source_view::LocalFetchResult,
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
    let asked = query.ref_.clone();
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
    let named = found.is_some();
    let mut failure = errors::failure(error, query, found);
    // The message names the ref as asked; the leads pin its commit.
    if named {
        failure.message = errors::path_not_found(
            &errors::repo_path(query),
            asked.as_ref().map(|r| r.as_str()),
        );
    }
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
        .get_file_bytes(&content_request, request_context)
        .await?;
    // The one text rule localFetch reads by: binary is refused here, once.
    crate::tools::gh_shared::text::admit_read(&acquired)?;
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
    fn error_hint(&self, code: ErrorCode, _query: &serde_json::Value) -> Option<&'static str> {
        // The message names the item; `repair` is the corrected read.
        (code == ErrorCode::InvalidPattern).then_some(crate::response::output::NO_HINT)
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
    fn reads_source(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
