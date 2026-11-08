//! A repository's published security advisories as GitHub lists them: every
//! page walked once, each advisory read as an OSV-shaped record so one parser
//! ([`record::advisory`]) states it, and merged with OSV's record of the same
//! advisory by id or alias. A monorepo's advisory names several packages: the
//! row's own package keeps its entries and the others move to `alsoAffects`.
use super::Net;
use super::osv::CONCURRENT_READS;
use super::record::{self, SecurityAdvisory, Selection};
use super::repository::{RepoFailure, RepositoryRead, Visibility};
use super::source::{self, Target};
use crate::contracts::security::{SecurityContract, security_contract};
use crate::providers::artifact::ArtifactError;
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use octocode_github::{AdvisoryPage, RepositoryAdvisory};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::{future::Future, pin::Pin};

/// One page of a repository's advisories, or why there is none.
#[derive(Clone, Debug)]
pub(crate) enum AdvisoryRead {
    Page(AdvisoryPage),
    /// GitHub has no such repository (deleted, private, or never there).
    Missing,
    Failed(RepoFailure),
}

pub(crate) type AdvisoryFuture<'a> = Pin<Box<dyn Future<Output = AdvisoryRead> + Send + 'a>>;

/// How a walk ended.
#[derive(Debug)]
pub(super) enum End {
    /// GitHub linked no further page.
    Complete,
    Missing,
    /// A page was refused or malformed: what was read stays in
    /// [`Walked::advisories`], and the list is not whole.
    Failed(RepoFailure),
    /// Not walked: the list is compared with OSV's records of a commit, and
    /// OSV's walk had not reached its last page.
    Deferred,
}

#[derive(Debug)]
pub(super) struct Walked {
    pub advisories: Vec<RepositoryAdvisory>,
    pub end: End,
    /// Where a walk that stopped after reading a page reads on.
    pub resume: Option<SavedWalk>,
}

/// Where a stopped advisory walk reads on: GitHub's cursor of the page that
/// failed, and every cursor asked, so a cycle across calls still fails. The
/// pages before it were listed by the calls that read them.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SavedWalk {
    after: String,
    asked: BTreeSet<String>,
}

impl SavedWalk {
    /// The saved walk a `resume` names; only this tool issues one.
    pub(super) fn parse(token: &str) -> Result<Self, ArtifactError> {
        serde_json::from_str(token).map_err(|_| {
            ArtifactError::new(ErrorCode::InvalidInput, "Invalid saved advisory walk.")
        })
    }

    pub(super) fn token(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Every page of `owner/repo`'s advisories from the start, or from where
/// `from` stopped, each cursor asked once: a page that names a cursor already
/// asked is a cycle, which fails the walk rather than repeating its
/// advisories. A page that fails after this call read one saves where to read
/// on; one that fails first does not, as asking again is the same call.
pub(super) async fn walk(
    owner: &str,
    repo: &str,
    github: &dyn RepositoryRead,
    from: Option<&SavedWalk>,
) -> Walked {
    let mut advisories = Vec::new();
    let mut asked = from.map(|saved| saved.asked.clone()).unwrap_or_default();
    let mut after = from.map(|saved| saved.after.clone());
    let mut advanced = false;
    let stop = |advisories, end, after: Option<String>, asked, advanced| Walked {
        advisories,
        resume: match (&end, after) {
            (End::Failed(_), Some(after)) if advanced => Some(SavedWalk { after, asked }),
            _ => None,
        },
        end,
    };
    loop {
        let page = match github.advisories(owner, repo, after.as_deref()).await {
            AdvisoryRead::Page(page) => page,
            AdvisoryRead::Missing => return stop(advisories, End::Missing, None, asked, false),
            AdvisoryRead::Failed(failure) => {
                return stop(advisories, End::Failed(failure), after, asked, advanced);
            }
        };
        advanced = true;
        advisories.extend(page.advisories);
        let Some(next) = page.next else {
            return stop(advisories, End::Complete, None, asked, false);
        };
        if !asked.insert(next.clone()) {
            let cycle = End::Failed(RepoFailure {
                code: ErrorCode::Decode,
                why: "GitHub linked the same advisory page twice, so the list stopped there."
                    .into(),
            });
            return stop(advisories, cycle, None, asked, false);
        }
        after = Some(next);
    }
}

/// The advisory in OSV's record shape. A vulnerable range GitHub writes as
/// text stays as written under `ecosystem_specific`: no range is inferred from
/// it.
fn osv_shape(advisory: &RepositoryAdvisory, contract: &SecurityContract) -> Value {
    let affected = advisory
        .vulnerabilities
        .iter()
        .map(|entry| {
            let mut row = json!({});
            if let Some(package) = &entry.package {
                // An ecosystem the contract does not map stays as GitHub wrote it.
                let ecosystem = package.ecosystem.as_deref().map(|name| {
                    contract
                        .osv_ecosystem_of_github(name)
                        .unwrap_or(name)
                        .to_owned()
                });
                row["package"] = json!({
                    "ecosystem": ecosystem,
                    "name": package.name,
                });
            }
            row["ecosystem_specific"] = json!({
                "vulnerable_version_range": entry.vulnerable_version_range,
                "patched_versions": entry.patched_versions,
            });
            row
        })
        .collect::<Vec<_>>();
    let mut record = json!({
        "id": advisory.ghsa_id,
        "aliases": advisory.cve_id.iter().collect::<Vec<_>>(),
        "summary": advisory.summary,
        "details": advisory.description,
        "published": advisory.published_at,
        "modified": advisory.updated_at,
        "withdrawn": advisory.withdrawn_at,
        "affected": affected,
        "database_specific": {
            "severity": advisory.severity,
            "source": advisory.html_url,
            "cwe_ids": advisory.cwe_ids,
        },
    });
    if let Some(vector) = advisory
        .cvss
        .as_ref()
        .and_then(|cvss| cvss.vector_string.as_deref())
    {
        record["severity"] = json!([{ "type": cvss_type(vector), "score": vector }]);
    }
    record
}

/// OSV's name for a CVSS vector's version.
fn cvss_type(vector: &str) -> &'static str {
    match vector.split('/').next() {
        Some("CVSS:4.0") => "CVSS_V4",
        Some("CVSS:3.1" | "CVSS:3.0") => "CVSS_V3",
        _ => "CVSS_V2",
    }
}

/// The patched versions GitHub states, each once.
fn patched(advisory: &RepositoryAdvisory, own_package: impl Fn(&str) -> bool) -> Vec<String> {
    let mut versions: Vec<String> = Vec::new();
    for entry in &advisory.vulnerabilities {
        let named = entry
            .package
            .as_ref()
            .and_then(|package| package.name.as_deref());
        if !named.is_some_and(&own_package) {
            continue;
        }
        for version in entry
            .patched_versions
            .iter()
            .flat_map(|text| text.split(','))
        {
            let version = version.trim();
            if !version.is_empty() && !versions.iter().any(|known| known == version) {
                versions.push(version.to_owned());
            }
        }
    }
    versions
}

/// The advisory as a row of `selection`'s package; with every entry when the
/// selection is [`Selection::All`].
pub(super) fn row(
    advisory: &RepositoryAdvisory,
    selection: &Selection<'_>,
) -> Result<SecurityAdvisory, String> {
    let contract = security_contract().map_err(str::to_owned)?;
    let mut row = record::advisory(&osv_shape(advisory, contract), selection)?;
    row.link = advisory
        .html_url
        .clone()
        .unwrap_or_else(|| format!("https://github.com/advisories/{}", advisory.ghsa_id));
    let own = row
        .affected
        .iter()
        .filter_map(|entry| entry["packageName"].as_str())
        .collect::<BTreeSet<_>>();
    row.fixed_in = patched(advisory, |name| own.contains(name));
    Ok(row)
}

/// The rows of GitHub's advisories among `rows` that share an id or alias with
/// one of `known` (OSV's records): the twin of an advisory OSV listed.
pub(super) fn twins(
    known: &[&SecurityAdvisory],
    rows: Vec<SecurityAdvisory>,
) -> Vec<SecurityAdvisory> {
    let ids = |row: &SecurityAdvisory| {
        std::iter::once(row.id.clone())
            .chain(row.aliases.iter().cloned())
            .collect::<BTreeSet<_>>()
    };
    let known_ids = known
        .iter()
        .flat_map(|row| ids(row))
        .collect::<BTreeSet<_>>();
    rows.into_iter()
        .filter(|row| ids(row).iter().any(|id| known_ids.contains(id)))
        .collect()
}

/// Why a walk's list is not whole; none: it is.
pub(super) fn failure(end: &End) -> Option<RepoFailure> {
    match end {
        End::Complete | End::Deferred => None,
        End::Missing => Some(RepoFailure {
            code: ErrorCode::NotFound,
            why: "GitHub has no advisory list for this repository.".into(),
        }),
        End::Failed(failure) => Some(failure.clone()),
    }
}

/// The sentence's clause for a finished list.
pub(super) fn count_clause(count: usize) -> String {
    match count {
        0 => "No published repository advisory".to_owned(),
        1 => "1 published repository advisory".to_owned(),
        _ => format!("{count} published repository advisories"),
    }
}

/// The sentence's clause for a list that stopped before its end.
pub(super) fn partial_count_clause(count: usize) -> String {
    format!(
        "{} read before GitHub stopped, so the list is not whole",
        count_clause(count)
    )
}

/// Why the GitHub source is partial: the failure of the page that stopped the
/// list, and what stays of it. A walk saved to read on says so; else a new call
/// reads the list again from the start.
pub(super) fn unread_why(read: usize, stopped: &RepoFailure, reads_on: bool) -> String {
    if read == 0 {
        return format!("Advisories not read: {}", stopped.why);
    }
    let then = if reads_on {
        "next.nextPage reads on from the page that failed."
    } else {
        "A new call reads the list again from the start."
    };
    format!(
        "Advisories read in part: {read} listed, the rest not read: {} {then}",
        stopped.why
    )
}

/// GitHub's advisories of a repository as rows of `selection`, and how the walk
/// ended. A record that does not read fails the call whole, so none is dropped.
pub(super) async fn github_rows(
    owner: &str,
    repo: &str,
    net: &Net<'_>,
    selection: &Selection<'_>,
    from: Option<&SavedWalk>,
) -> Result<(Vec<SecurityAdvisory>, End, Option<SavedWalk>), ArtifactError> {
    let Some(github) = net.github else {
        let failure = RepoFailure {
            code: ErrorCode::ProviderError,
            why: "No GitHub transport is available to this call.".into(),
        };
        return Ok((Vec::new(), End::Failed(failure), None));
    };
    let walked = walk(owner, repo, github, from).await;
    let target = format!("{owner}/{repo}");
    let rows = walked
        .advisories
        .iter()
        .map(|advisory| {
            row(advisory, selection).map_err(|reason| super::osv::malformed(&reason, &target))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((rows, walked.end, walked.resume))
}

/// What a repository's advisories read as.
pub(super) struct Listed {
    /// One brief row per advisory read, OSV's record merged with GitHub's by
    /// id or alias. When the walk did not finish these are the rows of the
    /// pages already read, GitHub's alone and not the whole list: `end` says so.
    pub advisories: Vec<Value>,
    pub end: End,
    /// The repository is not known to be public: its advisory ids were not sent
    /// to OSV, so each row stands on GitHub's record alone.
    pub osv_skipped: bool,
    /// Advisory ids OSV failed to answer for, with why: their rows stand on
    /// GitHub's record alone.
    pub osv_failed: Vec<(String, ArtifactError)>,
    /// Where a walk that stopped reads on.
    pub resume: Option<SavedWalk>,
}

/// The repository's advisories, each merged with OSV's record of the same
/// advisory, asked together. An advisory OSV does not list stands on GitHub's
/// record; so does one OSV failed to answer for, named in `osv_failed`. A
/// repository GitHub does not say is public never has its advisory ids sent to
/// OSV: the rows are GitHub's alone, and `osv_skipped` says so. A list that
/// stopped early keeps the rows read, GitHub's alone, and asks OSV nothing.
pub(super) async fn list(
    owner: &str,
    repo: &str,
    visibility: Visibility,
    net: &Net<'_>,
    selection: &Selection<'_>,
    from: Option<&SavedWalk>,
) -> Result<Listed, ArtifactError> {
    let (github, end, resume) = github_rows(owner, repo, net, selection, from).await?;
    let osv_skipped = !visibility.third_party_may_hear() && !github.is_empty();
    let ask_osv = matches!(end, End::Complete) && !osv_skipped;
    let asked = github.iter().filter(|_| ask_osv).map(|row| &row.id);
    let answers = stream::iter(asked)
        .map(|id| async move {
            let answer = source::ask_all(&Target::Advisory(id.clone()), net).await;
            (id, answer)
        })
        .buffered(CONCURRENT_READS)
        .collect::<Vec<_>>()
        .await;
    let mut osv = Vec::new();
    let mut osv_failed = Vec::new();
    for (id, answer) in answers {
        match answer {
            Ok(results) => osv.extend(results.into_iter().flat_map(|result| result.advisories)),
            Err(error) if error.code == ErrorCode::NotFound => {}
            Err(error) => osv_failed.push((id.clone(), error)),
        }
    }
    let rows = osv.iter().chain(&github).collect::<Vec<_>>();
    let advisories = record::advisory_groups(&rows)
        .iter()
        .map(|group| record::group_brief(group))
        .collect();
    Ok(Listed {
        advisories,
        end,
        osv_skipped,
        osv_failed,
        resume,
    })
}

#[cfg(test)]
#[path = "advisories_tests.rs"]
mod tests;
