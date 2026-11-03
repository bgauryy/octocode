//! Repository reads: owner listings, trees and metadata.
use serde::Deserialize;

use super::{RepositoryMetadata, RepositorySearchItem, TreeRequest, TreeResponse, decode};
use crate::{GitHubTransport, ProviderError, RequestContext, RequestSpec};

impl GitHubTransport {
    /// List an owner's repositories. `sort` is a list-API sort key
    /// (`created`, `updated`, `pushed`, `full_name`); when set, results are
    /// ordered descending (newest first) like the search API's date sorts.
    /// The token's own login lists every repository it owns, private ones
    /// included (`/user/repos`); another user's listing is public. The
    /// owner's route is resolved from this request's credential.
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
        // The route this request (or the continuation it resumes) found
        // stands for every later page: no second organization probe.
        let known = context
            .owner_route(owner)
            .and_then(|route| OwnerSource::named(&route));
        let response = match known {
            Some(source) => self.execute(fetch(source)?, context).await?,
            None => match self
                .execute(fetch(OwnerSource::Organization)?, context)
                .await
            {
                Ok(response) => {
                    context.record_owner_route(owner, OwnerSource::Organization.name());
                    response
                }
                Err(error) if error.status == Some(404) => {
                    let viewer = self.viewer_login(context).await;
                    // A transient viewer failure decides nothing to remember.
                    let definitive = !matches!(viewer, Viewer::Unknown);
                    let source = match viewer {
                        Viewer::Login(login) if login.eq_ignore_ascii_case(owner) => {
                            OwnerSource::Viewer
                        }
                        Viewer::Login(_) | Viewer::NoUser | Viewer::Unknown => OwnerSource::User,
                    };
                    let response = self.execute(fetch(source)?, context).await?;
                    if definitive {
                        context.record_owner_route(owner, source.name());
                    }
                    response
                }
                Err(error) => return Err(error),
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
        let metadata: RepositoryMetadata = self.repository_as(owner, repo, context).await?;
        // The read names the canonical repository for the rest of this
        // request: a rename check after it sends nothing more.
        if let Some(full_name) = metadata.full_name.as_deref() {
            context.record_full_name(owner, repo, full_name);
        }
        Ok(metadata)
    }

    /// The request token's user. Without a token, or for a
    /// token GitHub says has no user (an app token: 401/403/404), there is
    /// none; a transient failure is `Unknown`.
    async fn viewer_login(&self, context: &RequestContext) -> Viewer {
        #[derive(Deserialize)]
        struct User {
            login: String,
        }
        if context.resolved_credential().is_none() {
            return Viewer::NoUser;
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
        let item: RepositorySearchItem = self.repository_as(owner, repo, context).await?;
        context.record_full_name(owner, repo, &item.full_name);
        Ok(item)
    }

    /// `repos/{owner}/{repo}` decoded directly as `T`.
    async fn repository_as<T: for<'de> Deserialize<'de>>(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<T, ProviderError> {
        let url = self.endpoint().rest(&["repos", owner, repo])?;
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
        decode(
            page.body.as_ref(),
            "invalid GitHub repository metadata response",
        )
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

impl OwnerSource {
    const ALL: [Self; 3] = [Self::Organization, Self::User, Self::Viewer];

    /// The route's name in a request's facts.
    fn name(self) -> &'static str {
        match self {
            Self::Organization => "orgs",
            Self::User => "users",
            Self::Viewer => "user",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.name() == name)
    }
}
