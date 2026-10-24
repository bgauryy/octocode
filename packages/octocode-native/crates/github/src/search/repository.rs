//! Repository reads: owner listings, trees, metadata, and the memos that share them.
use serde::Deserialize;
use std::time::{Duration, Instant};

use super::{RepositoryMetadata, RepositorySearchItem, TreeRequest, TreeResponse, decode};
use crate::{GitHubTransport, ProviderError, RequestContext, RequestSpec};

impl GitHubTransport {
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
        order: &str,
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
                    .append_pair("direction", order);
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
                    error.with_reason(crate::ProviderErrorReason::RepositoryNotFound)
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
    sources: octocode_io::memo::Memo<(crate::Partition, String), OwnerSource>,
    viewers: octocode_io::memo::Memo<crate::Partition, Option<String>>,
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
    fn get(&self, key: &(crate::Partition, String)) -> Option<OwnerSource> {
        self.sources.get(key).map(|hit| *hit.value)
    }

    fn set(&self, key: (crate::Partition, String), source: OwnerSource) {
        self.sources.insert(key, source, 1, None);
    }
}

/// How long a read repository stays shared: long enough for one batch.
const REPOSITORY_MEMO_TTL: Duration = Duration::from_secs(30);

/// The latest body of each `(credential, owner/repo)` read: a retained
/// flight, whose slot is both the single flight and the memo.
pub(crate) type RepositoryMemo =
    crate::KeyedFlight<(crate::Partition, String), Option<(Instant, bytes::Bytes)>>;

/// Repository keys kept before idle ones are dropped.
pub(crate) const REPOSITORY_MEMO_KEYS: usize = 256;
