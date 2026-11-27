//! What securityCheck reads of a repository beyond its facts: one commit and
//! where it lives (branches, pull requests, tags, the default branch), and the
//! repository's published security advisories, page by page.
use super::{
    GitHubTransport, NamedRef, ProviderError, ProviderErrorKind, RefKind, RequestContext,
    RequestSpec,
};
use crate::refs::repository_missing;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};

/// How many branch comparisons run at once: a concurrency, never a limit on
/// how many branches are compared.
const COMPARES_IN_FLIGHT: usize = 8;

/// GitHub's own statement of a commit: the signature check it ran.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRead {
    pub verified: bool,
    pub verification_reason: Option<String>,
}

/// A branch or tag whose history holds a commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefHolder {
    pub kind: RefKind,
    pub name: String,
}

/// The branches, or else the tags, whose history holds a commit, as found
/// within one call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RefHolders {
    /// Every holder found, in name order. Branches are walked before tags and
    /// the walk ends after the page that finds a holder, so the list is of one
    /// kind; empty: none holds it.
    pub holders: Vec<RefHolder>,
    /// Every branch and tag was compared. False when the walk ended after the
    /// page that found a holder with refs left unread, or a comparison failed:
    /// more refs may hold the commit.
    pub complete: bool,
    /// The comparison that failed beside the holders found, first by ref
    /// order: why the list may be short, besides refs left unread.
    pub failed: Option<ProviderError>,
}

/// One page's comparisons, as they finished: each ref's place in the page, its
/// name and whether its history holds the commit.
pub(crate) type Compared = Vec<(usize, String, Result<bool, ProviderError>)>;

/// Adds every holder of `compared` to `found`, in the page's order, and keeps
/// in `failed` the first failure by that order, never by which comparison
/// finished first: the same refs give the same failure on every run.
pub(crate) fn settle(
    kind: RefKind,
    mut compared: Compared,
    (found, failed): (&mut Vec<RefHolder>, &mut Option<ProviderError>),
) {
    compared.sort_by_key(|(index, ..)| *index);
    for (_, name, held) in compared {
        match held {
            Ok(true) => found.push(RefHolder { kind, name }),
            Ok(false) => {}
            Err(error) => *failed = failed.take().or(Some(error)),
        }
    }
}

/// The names of the tags in `tips` that point at `sha`, in any letter case.
pub fn tags_pointing_at(tips: &[NamedRef], sha: &str) -> Vec<String> {
    tips.iter()
        .filter(|tag| tag.sha.eq_ignore_ascii_case(sha))
        .map(|tag| tag.name.clone())
        .collect()
}

/// One security advisory as GitHub publishes it: a repository's own
/// (`/repos/{o}/{r}/security-advisories`) or one of its global database
/// (`/advisories`). Each field is the one either endpoint names; one that an
/// endpoint does not publish stays empty.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepositoryAdvisory {
    pub ghsa_id: String,
    pub cve_id: Option<String>,
    /// The record's API address.
    pub url: Option<String>,
    pub html_url: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub severity: Option<String>,
    /// A repository advisory's state (`published`, `withdrawn`, ...).
    pub state: Option<String>,
    /// The account that published a repository advisory.
    pub publisher: Option<AdvisoryUser>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub withdrawn_at: Option<String>,
    /// The global database's class of the record: `reviewed`, `unreviewed`
    /// or `malware` (absent on a repository's own advisory).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub cvss: Option<AdvisoryCvss>,
    /// Each CVSS version GitHub scored, as `cvss_v3` and `cvss_v4`.
    pub cvss_severities: Option<CvssSeverities>,
    /// GitHub's EPSS of the advisory (global database only).
    pub epss: Option<AdvisoryEpss>,
    #[serde(default)]
    pub cwe_ids: Vec<String>,
    #[serde(default)]
    pub cwes: Vec<AdvisoryCwe>,
    #[serde(default)]
    pub credits: Vec<AdvisoryCredit>,
    /// Reference URLs (global database only).
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub vulnerabilities: Vec<AdvisoryVulnerability>,
}

impl RepositoryAdvisory {
    /// Every CVSS vector GitHub states with the score it publishes beside it,
    /// each vector once: `cvss`, then each of `cvss_severities`. A severity with
    /// no vector is none (GitHub writes `score: 0.0` beside a null vector).
    #[must_use]
    pub fn cvss_all(&self) -> Vec<(String, Option<f64>)> {
        let severities = self.cvss_severities.as_ref();
        let mut found: Vec<(String, Option<f64>)> = Vec::new();
        for cvss in std::iter::once(self.cvss.as_ref())
            .chain(severities.map(|all| all.cvss_v3.as_ref()))
            .chain(severities.map(|all| all.cvss_v4.as_ref()))
            .flatten()
        {
            if let Some(vector) = cvss.vector_string.as_deref()
                && !found.iter().any(|(known, _)| known == vector)
            {
                found.push((vector.to_owned(), cvss.score));
            }
        }
        found
    }

    /// Each credited account's login and GitHub's credit type: a repository
    /// advisory names `login`, the global database a `user`.
    #[must_use]
    pub fn credit_logins(&self) -> Vec<(String, Option<String>)> {
        self.credits
            .iter()
            .filter_map(|credit| {
                let login = credit
                    .login
                    .clone()
                    .or_else(|| credit.user.as_ref().and_then(|user| user.login.clone()))?;
                Some((login, credit.kind.clone()))
            })
            .collect()
    }

    /// Every CWE id the advisory names, once each: `cwe_ids`, then `cwes`.
    #[must_use]
    pub fn cwe_list(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for id in self
            .cwe_ids
            .iter()
            .chain(self.cwes.iter().filter_map(|cwe| cwe.cwe_id.as_ref()))
        {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdvisoryCvss {
    pub vector_string: Option<String>,
    /// The base score GitHub publishes with the vector.
    pub score: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CvssSeverities {
    pub cvss_v3: Option<AdvisoryCvss>,
    pub cvss_v4: Option<AdvisoryCvss>,
}

/// GitHub's EPSS: the probability of exploitation in the next 30 days and its
/// percentile among scored CVEs, both 0 to 1.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct AdvisoryEpss {
    pub percentage: f64,
    pub percentile: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdvisoryUser {
    pub login: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdvisoryCwe {
    pub cwe_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdvisoryCredit {
    pub login: Option<String>,
    pub user: Option<AdvisoryUser>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// One package an advisory names, with its affected and patched versions.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdvisoryVulnerability {
    pub package: Option<AdvisoryPackage>,
    pub vulnerable_version_range: Option<String>,
    /// A repository advisory's patched versions, comma-separated.
    pub patched_versions: Option<String>,
    /// The global database's first patched version.
    pub first_patched_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdvisoryPackage {
    pub ecosystem: Option<String>,
    pub name: Option<String>,
}

/// One provider page of advisories and the cursor of the page that follows.
#[derive(Clone, Debug)]
pub struct AdvisoryPage {
    pub advisories: Vec<RepositoryAdvisory>,
    /// GitHub's `after` cursor of the next page; none: this was the last.
    pub next: Option<String>,
}

fn decode<T: for<'de> Deserialize<'de>>(body: &[u8], what: &str) -> Result<T, ProviderError> {
    serde_json::from_slice(body).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::Decode,
            format!("invalid GitHub {what} response"),
        )
    })
}

impl GitHubTransport {
    /// The commit `sha` of a repository. `None`: the repository has no such
    /// commit (GitHub answers 422 for a hash it cannot resolve); a missing
    /// repository is `RepositoryNotFound`.
    pub async fn commit_read(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        context: &RequestContext,
    ) -> Result<Option<CommitRead>, ProviderError> {
        #[derive(Deserialize)]
        struct Row {
            commit: Inner,
        }
        #[derive(Deserialize)]
        struct Inner {
            verification: Option<Verification>,
        }
        #[derive(Deserialize)]
        struct Verification {
            verified: bool,
            reason: Option<String>,
        }
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "commits", sha])?;
        match self.execute(RequestSpec::get(url), context).await {
            Ok(page) => {
                let row: Row = decode(&page.body, "commit")?;
                Ok(Some(match row.commit.verification {
                    Some(v) => CommitRead {
                        verified: v.verified,
                        verification_reason: v.reason,
                    },
                    None => CommitRead {
                        verified: false,
                        verification_reason: None,
                    },
                }))
            }
            Err(error) if error.status == Some(422) => Ok(None),
            Err(error) => Err(repository_missing(error)),
        }
    }

    /// The branches whose head is `sha`.
    pub async fn commit_branch_heads(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        context: &RequestContext,
    ) -> Result<Vec<String>, ProviderError> {
        #[derive(Deserialize)]
        struct Branch {
            name: String,
        }
        let rows: Vec<Branch> = self
            .list_all(
                &["repos", owner, repo, "commits", sha, "branches-where-head"],
                "branches-where-head",
                context,
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.name).collect())
    }

    /// The numbers of the pull requests that contain `sha`.
    pub async fn commit_pull_numbers(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        context: &RequestContext,
    ) -> Result<Vec<u64>, ProviderError> {
        #[derive(Deserialize)]
        struct Pull {
            number: u64,
        }
        let rows: Vec<Pull> = self
            .list_all(
                &["repos", owner, repo, "commits", sha, "pulls"],
                "commit pull requests",
                context,
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.number).collect())
    }

    /// Every tag and the commit it points at, reading every tag page: `/tags`
    /// names the commit an annotated tag peels to, so a tag object needs no
    /// second read. Listed once, the tips serve both `tags_pointing_at` and
    /// `commit_in_other_ref`.
    pub async fn tag_tips(
        &self,
        owner: &str,
        repo: &str,
        context: &RequestContext,
    ) -> Result<Vec<NamedRef>, ProviderError> {
        self.all_refs(owner, repo, RefKind::Tags, context).await
    }

    /// Whether `sha` is on the history of `base` (a branch): GitHub's compare
    /// says `identical` or `behind`. Two existing commits with no common
    /// ancestor are not related: GitHub answers that with a 404.
    pub async fn commit_is_on(
        &self,
        owner: &str,
        repo: &str,
        base: &str,
        sha: &str,
        context: &RequestContext,
    ) -> Result<bool, ProviderError> {
        #[derive(Deserialize)]
        struct Comparison {
            status: String,
        }
        let range = format!("{base}...{sha}");
        let mut url = self
            .endpoint()
            .rest(&["repos", owner, repo, "compare", &range])?;
        url.query_pairs_mut().append_pair("per_page", "1");
        match self.execute(RequestSpec::get(url), context).await {
            Ok(page) => {
                let comparison: Comparison = decode(&page.body, "comparison")?;
                Ok(matches!(comparison.status.as_str(), "identical" | "behind"))
            }
            Err(error) if error.status == Some(404) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// The branches other than `skip_branch` (the default branch, already
    /// compared), or else the tags in `tags` (listed by the caller, once),
    /// whose history holds `sha`: GitHub's compare of the ref's tip with `sha`
    /// says `identical` or `behind`, the same read for both kinds. Branches are
    /// listed page by page and compared by their tip hash, so a name with a
    /// slash is never a route. Each page (a hundred refs) is compared whole, so
    /// every holder of the page that finds one is named, in name order, however
    /// the comparisons finish; the walk ends there, and `complete` says whether
    /// refs were left unread. No holder is proof only when every branch and tag
    /// was compared: a comparison that failed and found no holder fails the
    /// call.
    pub async fn commit_in_other_ref(
        &self,
        (owner, repo): (&str, &str),
        skip_branch: &str,
        tags: &[NamedRef],
        sha: &str,
        context: &RequestContext,
    ) -> Result<RefHolders, ProviderError> {
        let mut holders = Vec::new();
        let mut failed = None;
        let mut left_unread = false;
        'walk: {
            for page in 1_usize.. {
                let listed = self
                    .repository_refs(
                        owner,
                        repo,
                        RefKind::Branches,
                        page,
                        crate::MAX_PER_PAGE,
                        context,
                    )
                    .await?;
                let others = listed
                    .refs
                    .into_iter()
                    .filter(|named| named.name != skip_branch);
                self.compare_refs(
                    (owner, repo),
                    RefKind::Branches,
                    others,
                    sha,
                    context,
                    (&mut holders, &mut failed),
                )
                .await;
                if !holders.is_empty() {
                    left_unread = listed.has_more || !tags.is_empty();
                    break 'walk;
                }
                if !listed.has_more {
                    break;
                }
            }
            let mut pages = tags.chunks(crate::MAX_PER_PAGE).peekable();
            while let Some(page) = pages.next() {
                self.compare_refs(
                    (owner, repo),
                    RefKind::Tags,
                    page.iter().cloned(),
                    sha,
                    context,
                    (&mut holders, &mut failed),
                )
                .await;
                if !holders.is_empty() {
                    left_unread = pages.peek().is_some();
                    break 'walk;
                }
            }
        }
        if holders.is_empty() {
            return failed.map_or(
                Ok(RefHolders {
                    holders,
                    complete: true,
                    failed: None,
                }),
                Err,
            );
        }
        holders.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(RefHolders {
            holders,
            complete: !left_unread && failed.is_none(),
            failed,
        })
    }

    /// Compares each of `refs` (all of one `kind`) with `sha`, adding every
    /// holder to `found`. A comparison that fails is kept in `failed` and the
    /// rest go on: a holder found still proves the commit is held.
    async fn compare_refs(
        &self,
        (owner, repo): (&str, &str),
        kind: RefKind,
        refs: impl Iterator<Item = NamedRef>,
        sha: &str,
        context: &RequestContext,
        (found, failed): (&mut Vec<RefHolder>, &mut Option<ProviderError>),
    ) {
        let compared = stream::iter(refs.enumerate())
            .map(|(index, named)| async move {
                let held = self
                    .commit_is_on(owner, repo, &named.sha, sha, context)
                    .await;
                (index, named.name, held)
            })
            .buffer_unordered(COMPARES_IN_FLIGHT)
            .collect::<Vec<_>>()
            .await;
        settle(kind, compared, (found, failed));
    }

    /// One page of a repository's published security advisories, from the
    /// start or from the `after` cursor a page named.
    pub async fn repository_advisories_page(
        &self,
        owner: &str,
        repo: &str,
        after: Option<&str>,
        context: &RequestContext,
    ) -> Result<AdvisoryPage, ProviderError> {
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "security-advisories"])?;
        let filters = [("state", "published")];
        self.advisory_page(url, &filters, after, context, "repository advisories")
            .await
            .map_err(repository_missing)
    }

    /// One page of GitHub's global advisory database for `affects` (`name` or
    /// `name@version`) in `ecosystem` (GitHub's name), from the start or from
    /// the `after` cursor a page named. GitHub decides which advisories affect
    /// the version.
    pub async fn global_advisories_page(
        &self,
        ecosystem: &str,
        affects: &str,
        after: Option<&str>,
        context: &RequestContext,
    ) -> Result<AdvisoryPage, ProviderError> {
        let url = self.endpoint().rest(&["advisories"])?;
        let filters = [("ecosystem", ecosystem), ("affects", affects)];
        self.advisory_page(url, &filters, after, context, "global advisories")
            .await
    }

    /// One advisory of GitHub's global database by its GHSA id; `None`: GitHub
    /// has no such advisory.
    pub async fn global_advisory(
        &self,
        ghsa_id: &str,
        context: &RequestContext,
    ) -> Result<Option<RepositoryAdvisory>, ProviderError> {
        let url = self.endpoint().rest(&["advisories", ghsa_id])?;
        match self.execute(RequestSpec::get(url), context).await {
            Ok(page) => decode(&page.body, "global advisory").map(Some),
            Err(error) if error.status == Some(404) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// One of a repository's own advisories by its GHSA id, published or not
    /// yet reviewed into the global database; `None`: the repository has no
    /// such advisory. A missing repository is `RepositoryNotFound`.
    pub async fn repository_advisory(
        &self,
        owner: &str,
        repo: &str,
        ghsa_id: &str,
        context: &RequestContext,
    ) -> Result<Option<RepositoryAdvisory>, ProviderError> {
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "security-advisories", ghsa_id])?;
        match self.execute(RequestSpec::get(url), context).await {
            Ok(page) => decode(&page.body, "repository advisory").map(Some),
            Err(error) if error.status == Some(404) => {
                // GitHub answers 404 for both; the repository read tells them apart.
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// One page of advisories at `url`, `per_page=100` with `filters`, from
    /// `after`. The next
    /// cursor is read from GitHub's `Link`; a link with none is a malformed
    /// answer, never the end.
    async fn advisory_page(
        &self,
        mut url: url::Url,
        filters: &[(&str, &str)],
        after: Option<&str>,
        context: &RequestContext,
        what: &str,
    ) -> Result<AdvisoryPage, ProviderError> {
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("per_page", &crate::MAX_PER_PAGE.to_string());
            for (key, value) in filters {
                pairs.append_pair(key, value);
            }
            if let Some(after) = after {
                pairs.append_pair("after", after);
            }
        }
        let response = self.execute(RequestSpec::get(url), context).await?;
        let next = match &response.next {
            None => None,
            Some(link) => Some(
                link.query_pairs()
                    .find(|(key, _)| key == "after")
                    .map(|(_, cursor)| cursor.into_owned())
                    .ok_or_else(|| {
                        ProviderError::new(
                            ProviderErrorKind::Decode,
                            "GitHub linked a next advisory page with no cursor",
                        )
                    })?,
            ),
        };
        Ok(AdvisoryPage {
            advisories: decode(&response.body, what)?,
            next,
        })
    }
}
