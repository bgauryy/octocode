//! securityCheck's repository read through a row's GitHub transport: the same
//! credential path and budget as the release-tag checks.
use super::{GitHubServices, ResponseAccess};
use octocode_github::{
    ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext, tags_pointing_at,
};
use octocode_runtime_base::execution::ExecutionContext;
use octocode_tool_packages::security_check::{
    AdvisoryFuture, AdvisoryRead, CommitFacts, CommitFuture, CommitLookup, GhsaFuture, GhsaRead,
    OtherRefs, RepoFacts, RepoFailure, RepoFuture, RepoRead, RepositoryRead, Unread, Visibility,
};

pub(crate) struct RepositoryReader<'a> {
    pub(super) services: &'a GitHubServices,
    pub(super) context: &'a ExecutionContext,
    pub(super) access: &'a ResponseAccess,
    /// A request built here carried no credential.
    pub(super) anonymous: std::sync::atomic::AtomicBool,
}

/// What a refused GitHub read says: its code, and when to ask again. A rate
/// limit names its reset time; a refused credential names the sign-in; other failures say to retry.
fn failure_of(error: &ProviderError) -> RepoFailure {
    let code = error.code();
    let why = match (error.kind, error.rate_limit.as_deref()) {
        (ProviderErrorKind::RateLimited, Some(limit)) => {
            let reset = limit
                .reset_epoch_seconds
                .and_then(|secs| i64::try_from(secs).ok())
                .map(octocode_runtime_base::civil_date::iso8601_secs)
                .or_else(|| {
                    limit
                        .retry_after_seconds
                        .map(|secs| format!("{secs} seconds from now"))
                });
            match reset {
                Some(reset) => format!(
                    "GitHub rate limit reached ({code}); it resets at {reset}. Run `octocode auth login` for a higher quota."
                ),
                None => format!(
                    "GitHub rate limit reached ({code}), with no reset time stated. Run `octocode auth login` for a higher quota."
                ),
            }
        }
        (ProviderErrorKind::Authentication | ProviderErrorKind::Permission, _) => format!(
            "GitHub did not answer ({code}); run `octocode auth login` to sign in with a token that can read it."
        ),
        _ => format!("GitHub did not answer ({code}); retry later."),
    };
    RepoFailure { code, why }
}

impl RepositoryReader<'_> {
    /// The row's request context, or the failure that kept it from being built.
    async fn request(&self) -> Result<RequestContext, RepoFailure> {
        let request = self
            .services
            .resolve_request(self.context, self.access)
            .await
            .map_err(|error| RepoFailure {
                code: octocode_contract_types::ErrorCode::ProviderError,
                why: error.message.to_string(),
            })?;
        if request.resolved_credential().is_none() {
            self.anonymous
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(request)
    }

    async fn commit_facts(
        &self,
        (owner, repo, sha): (&str, &str, &str),
        default_branch: &str,
    ) -> CommitLookup {
        let request = match self.request().await {
            Ok(request) => request,
            Err(failure) => return CommitLookup::Failed(failure),
        };
        let transport = &self.services.provider.transport;
        let signature = match transport.commit_read(owner, repo, sha, &request).await {
            Ok(Some(signature)) => signature,
            Ok(None) => return CommitLookup::Missing,
            Err(error) => return CommitLookup::Failed(failure_of(&error)),
        };
        let (heads, pulls, tags, on_default) = tokio::join!(
            transport.commit_branch_heads(owner, repo, sha, &request),
            transport.commit_pull_numbers(owner, repo, sha, &request),
            transport.tag_tips(owner, repo, &request),
            async {
                if default_branch.is_empty() {
                    return Ok(None);
                }
                transport
                    .commit_is_on(owner, repo, default_branch, sha, &request)
                    .await
                    .map(Some)
            },
        );
        let mut facts = CommitFacts {
            verified: signature.verified,
            reason: signature.verification_reason,
            ..CommitFacts::default()
        };
        let mut unread = |what: &'static str, error: ProviderError| {
            facts.unread.push(Unread {
                what,
                failure: failure_of(&error),
            });
        };
        let (heads, pulls, tags, on_default) = (
            heads.map_err(|error| unread("branch heads", error)).ok(),
            pulls.map_err(|error| unread("pull requests", error)).ok(),
            tags.map_err(|error| unread("tags", error)).ok(),
            on_default
                .map_err(|error| unread("default branch history", error))
                .ok(),
        );
        facts.heads = heads;
        facts.pulls = pulls;
        facts.tags = tags.as_deref().map(|tips| tags_pointing_at(tips, sha));
        facts.on_default = on_default.flatten();
        // Only a commit that no head, pull request, tag tip or default history
        // places needs the other branches and tags: the one read that can show
        // it is in none of them.
        if facts.unread.is_empty()
            && facts.on_default == Some(false)
            && facts.heads.as_ref().is_some_and(Vec::is_empty)
            && facts.pulls.as_ref().is_some_and(Vec::is_empty)
            && facts.tags.as_ref().is_some_and(Vec::is_empty)
        {
            let tips = tags.as_deref().unwrap_or_default();
            match transport
                .commit_in_other_ref((owner, repo), default_branch, tips, sha, &request)
                .await
            {
                Ok(found) if found.holders.is_empty() => facts.other_ref = OtherRefs::NoneHolds,
                Ok(found) => {
                    // A holder proves the commit is held; a comparison that
                    // failed beside it is named, so the short list has its reason.
                    if let Some(error) = &found.failed {
                        facts.unread.push(Unread {
                            what: "the history of some other branches and tags",
                            failure: failure_of(error),
                        });
                    }
                    facts.other_ref = OtherRefs::Holds(found);
                }
                Err(error) => facts.unread.push(Unread {
                    what: "the history of the other branches and tags",
                    failure: failure_of(&error),
                }),
            }
        }
        CommitLookup::Found(facts)
    }
}

impl RepositoryRead for RepositoryReader<'_> {
    fn commit<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        sha: &'a str,
        default_branch: &'a str,
    ) -> CommitFuture<'a> {
        Box::pin(self.commit_facts((owner, repo, sha), default_branch))
    }

    fn advisories<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        after: Option<&'a str>,
    ) -> AdvisoryFuture<'a> {
        Box::pin(async move {
            let request = match self.request().await {
                Ok(request) => request,
                Err(failure) => return AdvisoryRead::Failed(failure),
            };
            match self
                .services
                .provider
                .transport
                .repository_advisories_page(owner, repo, after, &request)
                .await
            {
                Ok(page) => AdvisoryRead::Page(page),
                Err(error) if error.reason == Some(ProviderErrorReason::RepositoryNotFound) => {
                    AdvisoryRead::Missing
                }
                Err(error) => AdvisoryRead::Failed(failure_of(&error)),
            }
        })
    }

    fn global_advisories<'a>(
        &'a self,
        ecosystem: &'a str,
        affects: &'a str,
        after: Option<&'a str>,
    ) -> AdvisoryFuture<'a> {
        Box::pin(async move {
            let request = match self.request().await {
                Ok(request) => request,
                Err(failure) => return AdvisoryRead::Failed(failure),
            };
            match self
                .services
                .provider
                .transport
                .global_advisories_page(ecosystem, affects, after, &request)
                .await
            {
                Ok(page) => AdvisoryRead::Page(page),
                Err(error) => AdvisoryRead::Failed(failure_of(&error)),
            }
        })
    }

    fn ghsa<'a>(&'a self, repo: Option<(&'a str, &'a str)>, id: &'a str) -> GhsaFuture<'a> {
        Box::pin(async move {
            let request = match self.request().await {
                Ok(request) => request,
                Err(failure) => return GhsaRead::Failed(failure),
            };
            let transport = &self.services.provider.transport;
            let read = match repo {
                Some((owner, name)) => {
                    transport
                        .repository_advisory(owner, name, id, &request)
                        .await
                }
                None => transport.global_advisory(id, &request).await,
            };
            match read {
                Ok(Some(advisory)) => GhsaRead::Found(Box::new(advisory)),
                Ok(None) => GhsaRead::Missing,
                Err(error) => GhsaRead::Failed(failure_of(&error)),
            }
        })
    }

    fn anonymous(&self) -> bool {
        self.anonymous.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn read<'a>(&'a self, owner: &'a str, repo: &'a str) -> RepoFuture<'a> {
        Box::pin(async move {
            let request = match self.request().await {
                Ok(request) => request,
                Err(failure) => return RepoRead::Failed(failure),
            };
            match self
                .services
                .provider
                .transport
                .repository(owner, repo, &request)
                .await
            {
                Ok(item) => {
                    let (named_owner, named) = item
                        .full_name
                        .split_once('/')
                        .unwrap_or((owner, &item.name));
                    let facts = RepoFacts {
                        owner: named_owner.to_owned(),
                        name: named.to_owned(),
                        url: item.html_url.clone(),
                        stars: item.stargazers_count,
                        forks: item.forks_count,
                        pushed_at: item.pushed_at.clone(),
                        archived: item.archived,
                        fork: item.fork,
                        visibility: Visibility::of(item.private, &item.visibility),
                        default_branch: item.default_branch.clone(),
                        license: item
                            .license
                            .as_ref()
                            .and_then(|license| license.spdx_id.clone()),
                    };
                    if request.followed_rename(owner, repo) {
                        RepoRead::Moved(facts)
                    } else {
                        RepoRead::Found(facts)
                    }
                }
                Err(error) if error.reason == Some(ProviderErrorReason::RepositoryNotFound) => {
                    RepoRead::Missing
                }
                Err(error) => RepoRead::Failed(failure_of(&error)),
            }
        })
    }
}
