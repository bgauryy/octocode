mod access;
pub mod errors;
pub mod leads;
mod lfs;
mod shape;

use octocode_contract_types::{ErrorCode, GhGetFileContentQuery};

use serde::Serialize;
use serde_json::Value;

use octocode_github::{ContentRequest, GitHubProvider, ProviderError, RequestContext};
use octocode_runtime_base::response::result::ToolData;
use octocode_runtime_base::security::scan::ContentScan;
use octocode_tools_shared::gh_shared::{GhFailure, locate_path, missing_path};
use shape::{Fetched, shape};

/// The ref a first-page read of one file resolves before it reads: where
/// the batch-wide ref warm-up starts.
pub struct WarmTarget {
    pub owner: String,
    pub repo: String,
    pub reference: Option<String>,
}

impl WarmTarget {
    /// Whether `other` names the same repository ref (owner and repository
    /// case-insensitively, the ref as spelled).
    pub fn same_ref(&self, other: &Self) -> bool {
        self.owner.eq_ignore_ascii_case(&other.owner)
            && self.repo.eq_ignore_ascii_case(&other.repo)
            && self.reference == other.reference
    }
}

/// The ref `query` resolves ([`execute`]), when it resolves one: a valid
/// first-page read of a path inside the repository at a branch, tag, short
/// SHA or the default branch (a full SHA needs no request). `None` for
/// anything else (a path the policy withholds included), which the row
/// handles itself.
pub fn warm_target(query: &Value, withholds: impl Fn(&str) -> bool) -> Option<WarmTarget> {
    let query: GhGetFileContentQuery = serde_json::from_value(query.clone()).ok()?;
    if access::progress(&query).is_some_and(|progress| progress.offset != 0)
        || access::reference(&query).is_some_and(octocode_github::is_full_sha)
    {
        return None;
    }
    let path = octocode_tools_shared::gh_shared::listing_path(access::path(&query)).ok()?;
    // A withheld path is refused by the row before any request.
    (!path.is_empty() && !withholds(&path)).then(|| WarmTarget {
        owner: access::owner(&query).to_owned(),
        repo: access::repo(&query).to_owned(),
        reference: access::reference(&query).map(str::to_owned),
    })
}

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
    pub content: octocode_tools_shared::source_view::LocalFetchResult,
    /// Commit the read was pinned to (a branch or default-branch ref resolves
    /// to its current SHA; continuations reuse it).
    pub commit_sha: String,
    /// The file a repository symlink at the read path points to: the bytes
    /// are that file's, judged by its name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_not_found: Option<bool>,
    #[serde(
        flatten,
        serialize_with = "octocode_runtime_base::response::result::serialize_calls"
    )]
    pub calls: Option<Value>,
}

/// What shapes a fetched body, owned, so a ranked read shapes it on a
/// blocking worker: the scanner `S` (production's `ContentSecurity`) with
/// the call's context (its cancellation) and the selection runtime a ranked
/// read judges with.
#[derive(Clone)]
pub struct Shaping<S> {
    pub security: S,
    pub context: octocode_runtime_base::execution::ExecutionContext,
    pub selection: octocode_tools_shared::selection::Runtime,
}

/// Run one ghGetFileContent row: the read, or the failure with its one
/// recovery (a missing path walks to the case-corrected file or the nearest
/// existing directory). A symlink is followed to its file; a target
/// `withholds` refuses comes back as `Ok(Err(target))`, with none of its
/// bytes asked for.
pub async fn run<S: ContentScan + Clone + Send + Sync + 'static>(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    request: Result<&RequestContext, ProviderError>,
    window: Option<usize>,
    shaping: Shaping<S>,
    withholds: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Result<ToolData, String>, GhFailure> {
    let resolved = resolved_at(query).map_err(|error| errors::failure(error, query, None, None))?;
    let query = resolved.as_ref().unwrap_or(query);
    let request_context = request.map_err(|error| errors::failure(error, query, None, None))?;
    let mut read_commit = None;
    let error = match execute(
        provider,
        query,
        request_context,
        window,
        shaping,
        withholds,
        &mut read_commit,
    )
    .await
    {
        Ok(Ok(result)) => {
            return file_read(result)
                .map(Ok)
                .map_err(|error| errors::failure(error, query, None, None));
        }
        Ok(Err(target)) => return Ok(Err(target)),
        Err(error) => error,
    };
    if error.reason == Some(octocode_github::ProviderErrorReason::Submodule) {
        let failure = errors::failure(error, query, None, None);
        let mut at = errors::repo_path(query);
        at.reference = read_commit.as_deref().or(at.reference);
        return Err(octocode_tools_shared::gh_shared::submodule_failure(
            provider,
            &at,
            request_context,
            failure,
        )
        .await);
    }
    // A path failure comes after its ref resolved: its recovery leads pin
    // that commit, as the read's own continuations do, and name a renamed
    // repository as it is now.
    let scope = octocode_tools_shared::gh_shared::read_failure_scope(
        provider,
        access::owner(query),
        access::repo(query),
        access::path(query),
        read_commit,
        error,
        request_context,
    )
    .await;
    let error = scope.error;
    let commit = scope.commit.as_deref();
    let asked = access::reference(query).map(str::to_owned);
    let canonical = scope.canonical.and_then(|(owner, repo)| {
        octocode_tools_shared::gh_shared::with_repo(query, &owner, &repo).ok()
    });
    let renamed = canonical.is_some();
    let query = canonical.as_ref().unwrap_or(query);
    let at = octocode_tools_shared::gh_shared::RepoPath {
        reference: commit.or(asked.as_deref()),
        ..errors::repo_path(query)
    };
    let found = if missing_path(&error, Some(at.path)) {
        locate_path(provider, &at, request_context)
            .await
            .map(|found| errors::Located { found, renamed })
    } else {
        None
    };
    let named = found.is_some();
    let mut failure = errors::failure(error, query, commit, found);
    // The message names the ref as asked; the leads pin its commit.
    if named {
        failure.message = errors::path_not_found(&at, asked.as_deref());
    }
    Err(octocode_tools_shared::gh_shared::with_read_recovery(
        provider,
        failure,
        octocode_runtime_base::id::ToolId::GhGetFileContent,
        query,
        access::owner(query),
        access::repo(query),
        request_context,
    )
    .await)
}

/// The query with its `path` as the repository names it
/// ([`octocode_tools_shared::gh_shared::listing_path`], the rule every GitHub tool
/// reads a repository path by), so the read, its continuations and its
/// recovery leads name one path; `None` when it already does. A path that
/// climbs above the root, or names only the root, is invalid input, refused
/// before any request.
fn resolved_at(
    query: &GhGetFileContentQuery,
) -> Result<Option<GhGetFileContentQuery>, ProviderError> {
    let path = octocode_tools_shared::gh_shared::listing_path(access::path(query))?;
    if path.is_empty() {
        return Err(ProviderError::new(
            octocode_github::ProviderErrorKind::Validation,
            format!(
                "Path \"{}\" names the repository root, not a file.",
                access::path(query)
            ),
        ));
    }
    if path == access::path(query) {
        return Ok(None);
    }
    let mut resolved = query.clone();
    access::set_path(&mut resolved, &path)?;
    Ok(Some(resolved))
}

/// The read's status and source identity, including its resolved commit.
fn file_read(result: GhGetFileContentResult) -> Result<ToolData, ProviderError> {
    let status = result.file.content.status.row_status();
    // The row flattens the shared read's result, so its judged content
    // stands where a local read's does.
    let judged = result.file.content.judged_leaves();
    let data = serde_json::to_value(&result).map_err(|error| {
        ProviderError::new(
            octocode_github::ProviderErrorKind::Decode,
            error.to_string(),
        )
    })?;
    Ok(ToolData {
        status,
        judged,
        ..ToolData::from(data)
    })
}

pub(crate) async fn execute<S: ContentScan + Clone + Send + Sync + 'static>(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    request_context: &RequestContext,
    window: Option<usize>,
    shaping: Shaping<S>,
    withholds: &(dyn Fn(&str) -> bool + Sync),
    read_commit: &mut Option<String>,
) -> Result<Result<GhGetFileContentResult, String>, ProviderError> {
    // Resolve the ref once, then read the body at
    // the immutable SHA.
    let sha = provider
        .resolve_reference(
            access::owner(query),
            access::repo(query),
            access::reference(query),
            request_context,
        )
        .await?;
    *read_commit = Some(sha.clone());
    let content_request = ContentRequest {
        owner: access::owner(query).to_owned(),
        repo: access::repo(query).to_owned(),
        path: access::path(query).to_owned(),
        reference: Some(sha.clone()),
    };
    let (acquired, symlink_target) = match provider
        .get_file_bytes_following_links(&content_request, request_context, withholds)
        .await?
    {
        Ok(read) => read,
        Err(target) => return Ok(Err(target)),
    };
    // The one text rule localFetch reads by: binary is refused here, once.
    octocode_tools_shared::gh_shared::text::admit_read(&acquired)?;
    // A renamed repository: later requests and every lead use the canonical
    // name (a redirect-free read of a never-redirected name costs nothing).
    let (renamed_warning, canonical) = octocode_tools_shared::gh_shared::canonical_query(
        provider,
        query,
        access::owner(query),
        access::repo(query),
        request_context,
    )
    .await?
    .unzip();
    let query = canonical.as_ref().unwrap_or(query);
    let fetched = Fetched {
        window,
        renamed_warning,
        symlink_target,
    };
    // Plain and ranked reads both decode, scan and shape source text. Keep
    // that synchronous work off the async worker that serves other requests.
    let worker_query = query.clone();
    let result = tokio::task::spawn_blocking(octocode_io::notice::carried(move || {
        shape(
            &worker_query,
            acquired,
            fetched,
            &shaping.security,
            &shaping.context,
            access::classified(&worker_query).then_some(&shaping.selection),
        )
    }))
    .await
    .map_err(|_| {
        ProviderError::new(
            octocode_github::ProviderErrorKind::Decode,
            "content worker failed",
        )
    })??;
    // When the file last changed is its history, one documented call away
    // (ghSearchHistory), never a lead of every read. Its kind follows from
    // its path, which the request states.
    Ok(Ok(result))
}

/// This tool's recovery for the shared response stages.
pub struct Output;
impl octocode_runtime_base::response::output::ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &serde_json::Value) -> Option<&'static str> {
        // The message names the item; `repair` is the corrected read. A
        // symbols read no outline can serve carries the plain read as its
        // related lead; the shared hint names the local filesystem tools,
        // which cannot read this file.
        matches!(
            code,
            ErrorCode::InvalidPattern
                | ErrorCode::SymbolsUnsupported
                | ErrorCode::LanguageUnsupported
        )
        .then_some(octocode_runtime_base::response::output::NO_HINT)
    }
}

#[cfg(test)]
pub mod tests;
