use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use super::{GitHubTransport, ProviderError, ProviderErrorKind, RequestContext, RequestSpec};

#[derive(Clone, Debug)]
pub struct CodeSearchRequest {
    pub include_fragments: bool,
    pub query: String,
    pub page: usize,
    pub per_page: usize,
}
#[derive(Clone, Debug)]
pub struct RepositorySearchRequest {
    pub query: String,
    pub sort: Option<String>,
    pub page: usize,
    pub per_page: usize,
}
#[derive(Clone, Debug)]
pub struct TreeRequest {
    pub owner: String,
    pub repo: String,
    pub reference: String,
    pub recursive: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CodeSearchPage {
    pub total_count: usize,
    #[serde(default)]
    pub incomplete_results: bool,
    pub items: Vec<CodeSearchItem>,
    /// Organizations whose results GitHub withheld because the token is not
    /// SAML-authorized for them (`x-github-sso: partial-results`). A page
    /// with any is never cached.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sso_partial_organizations: Vec<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CodeSearchItem {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub html_url: String,
    pub repository: SearchRepository,
    #[serde(default)]
    pub text_matches: Vec<TextMatch>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextMatch {
    #[serde(default)]
    pub fragment: String,
    #[serde(default)]
    pub matches: Vec<TextMatchPosition>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextMatchPosition {
    #[serde(default)]
    pub indices: Vec<i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchRepository {
    pub full_name: String,
    pub html_url: String,
    pub url: String,
    #[serde(default)]
    pub default_branch: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositorySearchPage {
    pub total_count: usize,
    #[serde(default)]
    pub incomplete_results: bool,
    pub items: Vec<RepositorySearchItem>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositorySearchItem {
    pub full_name: String,
    pub name: String,
    pub html_url: String,
    pub default_branch: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub stargazers_count: usize,
    #[serde(default)]
    pub forks_count: usize,
    #[serde(default)]
    pub open_issues_count: usize,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub topics: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub pushed_at: Option<String>,
    pub language: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<License>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub fork: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct License {
    pub spdx_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TreeResponse {
    pub sha: String,
    #[serde(default)]
    pub truncated: bool,
    pub tree: Vec<TreeEntry>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// The git file mode: a `blob` of mode `120000` is a symlink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    pub size: Option<u64>,
    pub sha: Option<String>,
    pub url: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositoryMetadata {
    pub default_branch: String,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

impl GitHubTransport {
    /// A code search that serves a stored page first. Production searches
    /// split it into [`Self::cached_code_search`] and
    /// [`Self::search_code_uncached`]; the tests read the whole path at once.
    #[cfg(test)]
    pub(crate) async fn search_code(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<CodeSearchPage, ProviderError> {
        self.code_search(request, context, Lookup::Store).await
    }

    /// The code search for a request [`Self::cached_code_search`] just
    /// missed: the store is read again only when an identical search in
    /// flight stored its page meanwhile.
    pub async fn search_code_uncached(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<CodeSearchPage, ProviderError> {
        self.code_search(request, context, Lookup::Missed).await
    }

    async fn code_search(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
        lookup: Lookup,
    ) -> Result<CodeSearchPage, ProviderError> {
        let (mut page, withheld) = self
            .cached_search(
                "github-code-search",
                self.code_search_spec(request)?,
                context,
                lookup,
                "invalid GitHub code search response",
                |page: &CodeSearchPage| !page.incomplete_results,
            )
            .await?;
        page.sso_partial_organizations = withheld;
        Ok(page)
    }

    /// The stored page a code search would return for `request`, without
    /// sending: a caller planning several searches counts window slots only
    /// for the ones the cache cannot answer.
    pub async fn cached_code_search(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<Option<CodeSearchPage>, ProviderError> {
        self.cached_page(
            "github-code-search",
            &self.code_search_spec(request)?,
            context,
        )
        .await
    }

    fn code_search_spec(&self, request: &CodeSearchRequest) -> Result<RequestSpec, ProviderError> {
        let mut url = self.endpoint().rest(&["search", "code"])?;
        url.query_pairs_mut()
            .append_pair("q", &request.query)
            .append_pair("page", &request.page.to_string())
            .append_pair("per_page", &request.per_page.to_string());
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static(if request.include_fragments {
                "application/vnd.github.v3.text-match+json"
            } else {
                "application/vnd.github+json"
            }),
        );
        Ok(spec)
    }
    pub async fn search_repositories(
        &self,
        request: &RepositorySearchRequest,
        context: &RequestContext,
    ) -> Result<RepositorySearchPage, ProviderError> {
        let mut url = self.endpoint().rest(&["search", "repositories"])?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("q", &request.query)
                .append_pair("page", &request.page.to_string())
                .append_pair("per_page", &request.per_page.to_string());
            // The wire `sort` is `field:direction`; the API takes two params.
            if let Some((field, order)) = request.sort.as_deref().and_then(|s| s.split_once(':')) {
                pairs.append_pair("sort", field).append_pair("order", order);
            }
        }
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        self.cached_search(
            "github-repo-search",
            spec,
            context,
            Lookup::Store,
            "invalid GitHub repository search response",
            |page: &RepositorySearchPage| !page.incomplete_results,
        )
        .await
        .map(|(page, _)| page)
    }

    /// A search-API read through the response cache. Search answers come
    /// from GitHub's search index, which already trails writes, so an
    /// identical search (same URL, `Accept`, endpoint and credential) is
    /// served from the cache until the store's volatile TTL ends: no request,
    /// so no search spacing wait and no search rate-limit spend. A page
    /// `complete` rejects (GitHub's `incomplete_results`) is not stored: that
    /// is exactly what a retry must not get back. Single flight: an identical
    /// search already in flight (a sibling row of the batch) is waited for,
    /// and its stored page answers this one, so the duplicate spends no
    /// search quota and no spacing wait. `lookup` says whether the caller
    /// already found the store empty.
    pub(crate) async fn cached_search<T: Serialize + for<'de> Deserialize<'de>>(
        &self,
        namespace: &str,
        spec: RequestSpec,
        context: &RequestContext,
        lookup: Lookup,
        invalid: &'static str,
        complete: impl Fn(&T) -> bool,
    ) -> Result<(T, Vec<u64>), ProviderError> {
        let (partition, key) = self.search_cache_slot(namespace, &spec, context)?;
        let ticket = search_flights().join((partition.clone(), key.clone()));
        let mut stored = context.budget.wait(ticket.turn()).await?;
        if (lookup == Lookup::Store || *stored)
            && let Some(page) = self.stored_page(&partition, &key).await
        {
            return Ok((page, Vec::new()));
        }
        let response = self
            .execute(spec, context)
            .await
            .map_err(unsearchable_repository)?;
        let page: T = decode(response.body.as_ref(), invalid)?;
        let withheld = response.sso_partial_organizations();
        if withheld.is_empty()
            && complete(&page)
            && let Ok(bytes) = serde_json::to_vec(&page)
        {
            self.cache
                .put(
                    &partition,
                    key,
                    super::CachedContent {
                        etag: None,
                        bytes: bytes.into(),
                        resolved_ref: String::new(),
                        has_more: false,
                    },
                    super::Retention::Revalidate,
                )
                .await;
            *stored = true;
        }
        Ok((page, withheld))
    }

    /// A stored search page for `spec`, when the store still holds one.
    async fn cached_page<T: for<'de> Deserialize<'de>>(
        &self,
        namespace: &str,
        spec: &RequestSpec,
        context: &RequestContext,
    ) -> Result<Option<T>, ProviderError> {
        let (partition, key) = self.search_cache_slot(namespace, spec, context)?;
        Ok(self.stored_page(&partition, &key).await)
    }

    /// The page stored in search slot `(partition, key)`.
    async fn stored_page<T: for<'de> Deserialize<'de>>(
        &self,
        partition: &super::Partition,
        key: &str,
    ) -> Option<T> {
        self.cache
            .get(partition, key)
            .await
            .and_then(|cached| serde_json::from_slice::<T>(&cached.bytes).ok())
    }

    /// The store slot of a search: the partition carries the endpoint and
    /// credential, the key the URL and `Accept`.
    fn search_cache_slot(
        &self,
        namespace: &str,
        spec: &RequestSpec,
        context: &RequestContext,
    ) -> Result<(super::Partition, String), ProviderError> {
        let partition = self.cache_partition(context, None)?;
        Ok((partition, super::cache_key::request_key(namespace, spec)))
    }

    /// List an owner's repositories. `sort` is a list-API sort key
    /// (`created`, `updated`, `pushed`, `full_name`); when set, results are
    /// ordered descending (newest first) like the search API's date sorts.
    /// The token's own login lists every repository it owns, private ones
    /// included (`/user/repos`); another user's listing is public. The
    /// owner's kind is learned once per credential, so later pages spend one
    /// request each.
    pub async fn list_owner_repositories(
        &self,
        owner: &str,
        sort: &str,
        page: usize,
        per_page: usize,
        context: &RequestContext,
    ) -> Result<(Vec<RepositorySearchItem>, bool), ProviderError> {
        let fetch = |source: OwnerSource| -> Result<RequestSpec, ProviderError> {
            let mut url = match source {
                OwnerSource::Organization => self.endpoint().rest(&["orgs", owner, "repos"])?,
                OwnerSource::User => self.endpoint().rest(&["users", owner, "repos"])?,
                OwnerSource::Viewer => self.endpoint().rest(&["user", "repos"])?,
            };
            {
                let mut pairs = url.query_pairs_mut();
                if source == OwnerSource::Viewer {
                    pairs
                        .append_pair("affiliation", "owner")
                        .append_pair("visibility", "all");
                }
                pairs
                    .append_pair("page", &page.to_string())
                    .append_pair("per_page", &per_page.to_string())
                    .append_pair("sort", sort)
                    .append_pair("direction", "desc");
            }
            Ok(RequestSpec::get(url))
        };
        let key = (
            self.cache_partition(context, None)?,
            owner.to_ascii_lowercase(),
        );
        let known = self.owners.get(&key);
        let response = match known {
            Some(source) => self.execute(fetch(source)?, context).await?,
            None => match self
                .execute(fetch(OwnerSource::Organization)?, context)
                .await
            {
                Ok(response) => {
                    self.owners.set(key, OwnerSource::Organization);
                    response
                }
                Err(e) if e.status == Some(404) => {
                    let (source, known) = match self.viewer_login(context).await {
                        Viewer::Login(login) if login.eq_ignore_ascii_case(owner) => {
                            (OwnerSource::Viewer, true)
                        }
                        Viewer::Login(_) | Viewer::NoUser => (OwnerSource::User, true),
                        // This page lists public repositories; the next one
                        // asks again rather than remembering a guess.
                        Viewer::Unknown => (OwnerSource::User, false),
                    };
                    let response = self.execute(fetch(source)?, context).await?;
                    if known {
                        self.owners.set(key, source);
                    }
                    response
                }
                Err(e) => return Err(e),
            },
        };
        let has_more = response.next.is_some();
        Ok((
            decode(
                response.body.as_ref(),
                "invalid GitHub owner repositories response",
            )?,
            has_more,
        ))
    }
    pub async fn get_tree(
        &self,
        request: &TreeRequest,
        context: &RequestContext,
    ) -> Result<TreeResponse, ProviderError> {
        let mut url = self.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "trees",
            &request.reference,
        ])?;
        if request.recursive {
            url.query_pairs_mut().append_pair("recursive", "1");
        }
        decode(
            self.execute(RequestSpec::get(url), context)
                .await?
                .body
                .as_ref(),
            "invalid GitHub tree response",
        )
    }
    pub async fn repository_metadata(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<RepositoryMetadata, ProviderError> {
        self.repository_as(owner, repo, context).await
    }

    /// The token's user, read once per credential. Without a token, or for a
    /// token GitHub says has no user (an app token: 401/403/404), there is
    /// none; a transient failure is `Unknown` and is not remembered.
    async fn viewer_login(&self, context: &RequestContext) -> Viewer {
        #[derive(Deserialize)]
        struct User {
            login: String,
        }
        if context.resolved_credential().is_none() {
            return Viewer::NoUser;
        }
        let Ok(partition) = self.cache_partition(context, None) else {
            return Viewer::Unknown;
        };
        if let Some(known) = self.owners.viewers.get(&partition) {
            return (*known.value).clone().map_or(Viewer::NoUser, Viewer::Login);
        }
        let Ok(url) = self.endpoint().rest(&["user"]) else {
            return Viewer::Unknown;
        };
        let login = match self.execute(RequestSpec::get(url), context).await {
            Ok(page) => match decode::<User>(page.body.as_ref(), "invalid GitHub user response") {
                Ok(user) => Some(user.login),
                Err(_) => return Viewer::Unknown,
            },
            Err(error) if matches!(error.status, Some(401 | 403 | 404)) => None,
            Err(_) => return Viewer::Unknown,
        };
        self.owners
            .viewers
            .insert(partition, login.clone(), 1, None);
        login.map_or(Viewer::NoUser, Viewer::Login)
    }

    /// One repository's search-row facts from `repos/{owner}/{repo}`: an
    /// exact lookup that spends no search request.
    pub async fn repository(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<RepositorySearchItem, ProviderError> {
        self.repository_as(owner, repo, context).await
    }

    /// `repos/{owner}/{repo}` decoded as `T`, read once for a batch: rows
    /// that ask about the same repository (a renamed-scope check and an
    /// empty-scope check, or sibling rows) share one request while it is
    /// in flight and for [`REPOSITORY_MEMO_TTL`] after. Failures are not
    /// kept, so each caller of a failing lookup sees the failure itself.
    async fn repository_as<T: for<'de> Deserialize<'de>>(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<T, ProviderError> {
        let url = self.endpoint().rest(&["repos", owner, repo])?;
        let key = (
            self.cache_partition(context, None)?,
            format!(
                "{}/{}",
                owner.to_ascii_lowercase(),
                repo.to_ascii_lowercase()
            ),
        );
        let ticket = self.repositories.join(key);
        let mut held = context.budget.wait(ticket.turn()).await?;
        if let Some((at, body)) = held.as_ref()
            && at.elapsed() < REPOSITORY_MEMO_TTL
        {
            return decode(body, "invalid GitHub repository metadata response");
        }
        let page = self
            .execute(RequestSpec::get(url), context)
            .await
            .map_err(|error| {
                if error.status == Some(404) {
                    error.with_reason(super::ProviderErrorReason::RepositoryNotFound)
                } else {
                    error
                }
            })?;
        let decoded = decode(
            page.body.as_ref(),
            "invalid GitHub repository metadata response",
        )?;
        *held = Some((Instant::now(), page.body));
        Ok(decoded)
    }
}

/// What a credential's `/user` read established.
enum Viewer {
    Login(String),
    /// No token, or a token with no user.
    NoUser,
    /// The read failed transiently; nothing is known.
    Unknown,
}

/// Where an owner's repositories are listed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerSource {
    Organization,
    /// Another user: GitHub lists only their public repositories.
    User,
    /// The token's own user: private repositories are listed too.
    Viewer,
}

/// Owners and credentials remembered before the least recently used go.
const OWNER_MEMO_ENTRIES: usize = 256;
const OWNER_MEMO: octocode_io::memo::MemoLimits = octocode_io::memo::MemoLimits {
    max_entries: OWNER_MEMO_ENTRIES,
    max_weight: OWNER_MEMO_ENTRIES,
    keep_newest: false,
};

/// Each `(credential, owner)`'s listing source, learned on its first page,
/// and each credential's own login.
pub(crate) struct OwnerSources {
    sources: octocode_io::memo::Memo<(super::Partition, String), OwnerSource>,
    viewers: octocode_io::memo::Memo<super::Partition, Option<String>>,
}

impl Default for OwnerSources {
    fn default() -> Self {
        Self {
            sources: octocode_io::memo::Memo::new(OWNER_MEMO),
            viewers: octocode_io::memo::Memo::new(OWNER_MEMO),
        }
    }
}

impl OwnerSources {
    fn get(&self, key: &(super::Partition, String)) -> Option<OwnerSource> {
        self.sources.get(key).map(|hit| *hit.value)
    }

    fn set(&self, key: (super::Partition, String), source: OwnerSource) {
        self.sources.insert(key, source, 1, None);
    }
}

/// The process's in-flight searches, one per `(credential, search slot)`.
/// Whether a search reads the store before it sends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Lookup {
    /// Serve a stored page when there is one.
    Store,
    /// The caller just found none: read again only when a search of this
    /// flight stored one meanwhile.
    Missed,
}

/// Search flights; a slot says whether a search of its wave stored a page.
fn search_flights() -> &'static super::KeyedFlight<(super::Partition, String), bool> {
    static FLIGHTS: super::KeyedFlight<(super::Partition, String), bool> =
        super::KeyedFlight::transient();
    &FLIGHTS
}

/// How long a read repository stays shared: long enough for one batch.
const REPOSITORY_MEMO_TTL: Duration = Duration::from_secs(30);

/// The latest body of each `(credential, owner/repo)` read: a retained
/// flight, whose slot is both the single flight and the memo.
pub(crate) type RepositoryMemo =
    super::KeyedFlight<(super::Partition, String), Option<(Instant, bytes::Bytes)>>;

/// Repository keys kept before idle ones are dropped.
pub(crate) const REPOSITORY_MEMO_KEYS: usize = 256;
/// A 422 "cannot be searched" names what GitHub could not search: "users and
/// repositories" is the `repo:`/`user:` scope (the repository was not found,
/// not a bad query); "users" alone is a person qualifier (`author:`,
/// `assignee:`, …) whose login does not exist or is not visible.
fn unsearchable_repository(mut error: ProviderError) -> ProviderError {
    if error.status == Some(422) && error.message.contains("cannot be searched") {
        error.kind = ProviderErrorKind::NotFound;
        let reason = if error.message.contains("repositories") {
            super::ProviderErrorReason::RepositoryNotFound
        } else {
            super::ProviderErrorReason::UserNotFound
        };
        error = error.with_reason(reason);
    }
    error
}

fn decode<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    message: &'static str,
) -> Result<T, ProviderError> {
    serde_json::from_slice(bytes)
        .map_err(|_| ProviderError::new(ProviderErrorKind::Decode, message))
}
