//! A ref that does not exist, named against the repository's default
//! branch. GitHub tools never fall back to it silently: the failure says
//! which branch the repository actually has and offers the same call at that
//! ref as a runnable lead.
use super::GhFailure;
use crate::id::ToolId;
use crate::response::result::restart_query;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use serde_json::Value;

/// `prefix:owner/repo`; owner and repository are case-insensitive cache keys.
pub(crate) fn repo_key(prefix: &str, owner: &str, repo: &str) -> String {
    format!(
        "{prefix}:{}/{}",
        owner.to_ascii_lowercase(),
        repo.to_ascii_lowercase()
    )
}

/// The default branch's name, memoized per repository for the cache's
/// volatile lifetime (like the HEAD commit memo beside it).
pub(crate) async fn default_branch(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Result<String, ProviderError> {
    let key = repo_key("repo-default-branch", owner, repo);
    let partition = provider.transport.cache_partition(context, None).ok();
    if let Some(partition) = &partition
        && let Some(cached) = provider.transport.cache.get(partition, &key).await
        && let Ok(branch) = std::str::from_utf8(&cached.bytes)
    {
        return Ok(branch.to_owned());
    }
    let metadata = provider
        .transport
        .repository_metadata(owner, repo, context)
        .await?;
    // The same read names the repository's canonical name: a renamed
    // repository costs no further request.
    if let Some(full_name) = metadata.full_name.as_deref() {
        super::remember_canonical(provider, owner, repo, full_name, context).await;
    }
    let branch = metadata.default_branch;
    if let Some(partition) = &partition {
        provider
            .transport
            .cache
            .put(
                partition,
                key,
                octocode_github::CachedContent {
                    etag: None,
                    bytes: branch.as_bytes().to_vec().into(),
                    resolved_ref: branch.clone(),
                    has_more: false,
                },
                octocode_github::Retention::Revalidate,
            )
            .await;
    }
    Ok(branch)
}

/// What a missing `ref` leaves to say: the repository exists and its default
/// branch is another ref than the one asked for.
pub(crate) struct DefaultBranchRetry {
    branch: String,
    lead: Value,
}

impl DefaultBranchRetry {
    /// The repository's default branch.
    pub(crate) fn branch(&self) -> &str {
        &self.branch
    }

    /// `message` and the default branch: `…; the default branch is "master".`
    pub(crate) fn message(&self, message: &str) -> String {
        format!("{message}; the default branch is \"{}\".", self.branch)
    }

    /// `next.atDefaultBranch`: the same call with `ref` set to the default
    /// branch.
    pub(crate) fn lead(self) -> (&'static str, Value) {
        ("atDefaultBranch", self.lead)
    }

    /// The failure's message names the default branch and its recovery
    /// leads with the retry (the refs listing stays beside it).
    pub(crate) fn apply(self, failure: &mut GhFailure) {
        failure.message = self.message(&failure.message);
        let (name, lead) = self.lead();
        let next = failure.calls.get_or_insert_with(|| Box::new(Value::Null));
        if !next.is_object() {
            **next = serde_json::json!({});
        }
        next[name] = lead;
    }
}

/// The retry for a call (`row`, its wire query) whose refs in `fields` did
/// not resolve in `owner/repo`: each set to the default branch. `None`
/// leaves the failure as it is: a field names no ref or a full commit SHA
/// (not a branch), the default branch is a ref that failed, or the
/// repository's metadata cannot be read (so the repository itself may be
/// the problem).
pub(crate) async fn default_branch_retry(
    provider: &GitHubProvider,
    tool: ToolId,
    mut row: Value,
    fields: &[&str],
    context: &RequestContext,
) -> Option<DefaultBranchRetry> {
    let text = |row: &Value, name: &str| row.get(name).and_then(Value::as_str).map(str::to_owned);
    let (owner, repo) = (text(&row, "owner")?, text(&row, "repo")?);
    let requested = fields
        .iter()
        .map(|field| text(&row, field))
        .collect::<Option<Vec<_>>>()?;
    if requested.is_empty()
        || requested
            .iter()
            .any(|reference| reference.is_empty() || octocode_github::is_full_sha(reference))
    {
        return None;
    }
    let branch = default_branch(provider, &owner, &repo, context)
        .await
        .ok()
        .filter(|branch| !branch.is_empty() && !requested.contains(branch))?;
    for field in fields {
        row[*field] = Value::from(branch.clone());
    }
    let lead = restart_query(tool, &row);
    Some(DefaultBranchRetry { branch, lead })
}

/// Which of `refs` (field, ref) do not resolve in `owner/repo`, when a call
/// naming several failed with one not-found answer that does not say which
/// (a comparison). Each named ref costs one SHA-only request; a full SHA is
/// not probed (it is not a branch). Empty when every ref resolves or the
/// repository itself does not (a probe fails for another reason).
pub(crate) async fn unresolved_refs<'a>(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    refs: &[(&'a str, &str)],
    context: &RequestContext,
) -> Vec<&'a str> {
    let mut missing = Vec::new();
    for (field, reference) in refs {
        if reference.is_empty() || octocode_github::is_full_sha(reference) {
            continue;
        }
        match provider
            .transport
            .commit_sha(owner, repo, reference, context)
            .await
        {
            Ok(_) => {}
            Err(error)
                if error.reason == Some(octocode_github::ProviderErrorReason::RefNotFound) =>
            {
                missing.push(*field);
            }
            Err(_) => return Vec::new(),
        }
    }
    missing
}

/// [`default_branch_retry`] for a typed query's failed ref.
pub(crate) async fn with_default_branch(
    provider: &GitHubProvider,
    mut failure: GhFailure,
    tool: ToolId,
    query: &impl serde::Serialize,
    context: &RequestContext,
) -> GhFailure {
    if failure.error.reason != Some(octocode_github::ProviderErrorReason::RefNotFound) {
        return failure;
    }
    let row = crate::response::result::query_row(query, &[]);
    if let Some(retry) = default_branch_retry(provider, tool, row, &["ref"], context).await {
        retry.apply(&mut failure);
    }
    failure
}
