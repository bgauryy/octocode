//! A commit of a GitHub repository, as GitHub states it: whether the commit
//! is there, its signature verdict, the branches whose head it is, the pull
//! requests and tags that contain it, and whether the default branch's
//! history holds it. Facts, never a verdict: a commit that resolves through a
//! repository but sits on none of its branches may live only in a fork.
use super::Net;
use super::advisories::{self, End};
use super::answer::typed;
use super::record::Selection;
use super::repository::{self, RepoFailure, RepoRead, read_github};
use super::source::{Source, SourceResult};
use crate::contracts::security::security_contract;
use crate::id::ToolId;
use crate::providers::artifact::ArtifactError;
use crate::response::paging;
use crate::response::result::Continuation;
use octocode_contract_types::{ErrorCode, SecurityCheckView};
use octocode_github::{RefHolders, RefKind};
use serde_json::{Map, Value, json};
use std::{future::Future, pin::Pin};

/// A read that GitHub refused after the commit itself was found.
#[derive(Clone, Debug)]
pub(crate) struct Unread {
    pub what: &'static str,
    pub failure: RepoFailure,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CommitFacts {
    /// GitHub's verification of the signature.
    pub verified: bool,
    pub reason: Option<String>,
    /// Branches whose head is the commit; none: not read.
    pub heads: Option<Vec<String>>,
    /// Pull requests that contain it; none: not read.
    pub pulls: Option<Vec<u64>>,
    /// Tags that point at it; none: not read.
    pub tags: Option<Vec<String>>,
    /// The default branch's history holds it; none: not read.
    pub on_default: Option<bool>,
    /// The history of the repository's other branches and tags; not checked
    /// unless the commit is on no head, pull request, tag tip or default history.
    pub other_ref: OtherRefs,
    pub unread: Vec<Unread>,
}

/// Whether branches other than the default, or tags, hold the commit in their
/// history.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum OtherRefs {
    #[default]
    NotChecked,
    /// These branches' or tags' history holds it; `complete` says whether
    /// the list is whole.
    Holds(RefHolders),
    /// Every other branch and every tag was compared and none holds it.
    NoneHolds,
}

#[derive(Clone, Debug)]
pub(crate) enum CommitLookup {
    Found(CommitFacts),
    /// The repository has no such commit.
    Missing,
    Failed(RepoFailure),
}

pub(crate) type CommitFuture<'a> = Pin<Box<dyn Future<Output = CommitLookup> + Send + 'a>>;

/// What GitHub said of the commit, with the repository read it followed.
pub(super) struct Read {
    pub repository: RepoRead,
    pub repository_status: &'static str,
    /// `None`: not asked, as the repository is missing or unread.
    pub commit: Option<CommitLookup>,
}

impl CommitFacts {
    /// On the head of no branch, in no pull request or tag, and in the history
    /// of no branch or tag (the default branch and every other ref were compared):
    /// reachable only through the fork network. A read that failed proves
    /// nothing, so it never counts as absence.
    pub(super) fn only_in_fork_network(&self) -> bool {
        self.unread.is_empty()
            && self.on_default == Some(false)
            && self.other_ref == OtherRefs::NoneHolds
            && self.heads.as_ref().is_some_and(Vec::is_empty)
            && self.pulls.as_ref().is_some_and(Vec::is_empty)
            && self.tags.as_ref().is_some_and(Vec::is_empty)
    }
}

pub(super) fn status(read: &Read) -> &'static str {
    match &read.commit {
        Some(CommitLookup::Found(_)) => "found",
        Some(CommitLookup::Missing) => "noneKnown",
        Some(CommitLookup::Failed(_)) => "failed",
        None => read.repository_status,
    }
}

/// Why the source is not whole: the failure of the commit read or of the
/// repository read, else each read left unread with its own reason.
pub(super) fn failure(read: &Read) -> Vec<RepoFailure> {
    match (&read.commit, &read.repository) {
        (Some(CommitLookup::Failed(failure)), _) | (None, RepoRead::Failed(failure)) => {
            vec![failure.clone()]
        }
        (Some(CommitLookup::Found(facts)), _) => facts
            .unread
            .iter()
            .map(|unread| RepoFailure {
                code: unread.failure.code,
                why: format!("{} not read: {}", unread.what, unread.failure.why),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The `facts` of the GitHub entry in `sources[]`.
pub(super) fn facts_block(commit: &CommitLookup) -> Option<Value> {
    match commit {
        CommitLookup::Missing => Some(json!({ "exists": false })),
        CommitLookup::Failed(_) => None,
        CommitLookup::Found(facts) => {
            let mut tags = facts.tags.clone();
            let mut branches = Vec::new();
            if let OtherRefs::Holds(found) = &facts.other_ref {
                for holder in &found.holders {
                    match holder.kind {
                        RefKind::Tags => tags.get_or_insert_default().push(holder.name.clone()),
                        RefKind::Branches => branches.push(holder.name.clone()),
                    }
                }
            }
            // A list GitHub did not answer is absent, never empty: empty says
            // it was read and holds none.
            let mut block = json!({
                "exists": true,
                "signature": { "verified": facts.verified },
            });
            if let Some(heads) = &facts.heads {
                block["heads"] = json!(heads);
            }
            if let Some(pulls) = &facts.pulls {
                block["pulls"] = json!(pulls.iter().map(u64::to_string).collect::<Vec<_>>());
            }
            if let Some(tags) = tags {
                block["inTag"] = json!(tags);
            }
            if let Some(reason) = &facts.reason {
                block["signature"]["reason"] = json!(reason);
            }
            if let Some(on_default) = facts.on_default {
                block["onDefaultBranch"] = json!(on_default);
            }
            if !branches.is_empty() {
                block["inBranch"] = json!(branches);
            }
            if let OtherRefs::Holds(found) = &facts.other_ref {
                block["inRefsComplete"] = json!(found.complete);
            }
            Some(block)
        }
    }
}

/// Names as the sentence lists them: each typed on its own, so the separator
/// survives.
fn typed_list(names: &[String]) -> String {
    names
        .iter()
        .map(|name| typed(name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `a`, `a or b`, `a, b or c`.
fn or_list(items: &[&str]) -> String {
    match items.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// The sentence's GitHub clause: typed values only.
pub(super) fn clause(owner: &str, repo: &str, read: &Read) -> String {
    let target = typed(&format!("{owner}/{repo}"));
    match &read.commit {
        Some(CommitLookup::Missing) => format!(" GitHub has no such commit in {target}."),
        Some(CommitLookup::Failed(_)) | None => match &read.repository {
            RepoRead::Missing => {
                format!(" GitHub has no repository {target}, so the commit was not read there.")
            }
            _ => format!(" GitHub did not answer for {target}: the commit was not read there."),
        },
        Some(CommitLookup::Found(facts)) => {
            let signature = if facts.verified {
                "signed and verified".to_owned()
            } else {
                format!(
                    "not verified ({})",
                    typed(facts.reason.as_deref().unwrap_or("unsigned"))
                )
            };
            let mut places = Vec::new();
            // The places read and found empty: the only absence the clause states.
            let mut none = Vec::new();
            match facts.heads.as_deref() {
                Some([]) => none.push("branch head"),
                Some(heads) => places.push(format!("head of {}", typed_list(heads))),
                None => {}
            }
            match facts.tags.as_deref() {
                Some([]) => none.push("tag"),
                Some(tags) => places.push(format!("in tag {}", typed_list(tags))),
                None => {}
            }
            match facts.pulls.as_deref() {
                Some([]) => none.push("pull request"),
                Some(pulls) => places.push(format!("in {} pull request(s)", pulls.len())),
                None => {}
            }
            if facts.on_default == Some(true) {
                places.push("in the default branch's history".to_owned());
            }
            if let OtherRefs::Holds(found) = &facts.other_ref {
                for (kind, one, many) in [
                    (RefKind::Branches, "branch", "branches"),
                    (RefKind::Tags, "tag", "tags"),
                ] {
                    let names = found
                        .holders
                        .iter()
                        .filter(|holder| holder.kind == kind)
                        .map(|holder| holder.name.clone())
                        .collect::<Vec<_>>();
                    if !names.is_empty() {
                        let noun = if names.len() == 1 { one } else { many };
                        places.push(format!("in the history of {noun} {}", typed_list(&names)));
                    }
                }
                if found.failed.is_some() {
                    // The failed comparison is named with the reads not read.
                    places.push("possibly of other branches or tags too".to_owned());
                } else if !found.complete {
                    places.push(
                        "possibly of other branches or tags too: the walk ended before all were compared"
                            .to_owned(),
                    );
                }
            }
            let mut parts = vec![signature];
            if places.is_empty() && !none.is_empty() {
                parts.push(format!("found on no {}", or_list(&none)));
            } else if !places.is_empty() {
                parts.push(places.join(", "));
            }
            let mut said = parts.join(", ");
            if !facts.unread.is_empty() {
                let unread = facts
                    .unread
                    .iter()
                    .map(|unread| unread.what)
                    .collect::<Vec<_>>();
                said.push_str(&format!("; {} not read", unread.join(", ")));
            }
            format!(" GitHub has this commit in {target}: {said}.")
        }
    }
}

/// Everything GitHub said of the commit's repository for one row.
pub(super) struct Github {
    pub read: Read,
    /// Ids of the repository's advisories that OSV does not list for this
    /// commit: the repository's, not known to affect it.
    pub others: Vec<String>,
    pub advisories: End,
    /// How many of the repository's advisories were read, whole list or not.
    pub advisories_read: usize,
}

/// Reads the repository and the commit in it. Once OSV's walk is complete
/// (`osv_complete`), also the repository's advisories: one that shares an id or
/// alias with an OSV record of this commit is merged into it, in `osv`, and the
/// rest are named, never counted as affecting it. Before that, the list is not
/// compared: an advisory OSV lists on a page not yet read is not "not known to
/// affect" the commit.
pub(super) async fn read_all(
    (owner, repo, sha): (&str, &str, &str),
    osv: &mut Vec<SourceResult>,
    osv_complete: bool,
    net: &Net<'_>,
) -> Result<Github, ArtifactError> {
    let (repository, repository_status) = read_github(owner, repo, net).await;
    let (RepoRead::Found(facts) | RepoRead::Moved(facts)) = &repository else {
        return Ok(Github {
            read: Read {
                repository,
                repository_status,
                commit: None,
            },
            others: Vec::new(),
            advisories: End::Complete,
            advisories_read: 0,
        });
    };
    let commit = match net.github {
        Some(github) => github.commit(owner, repo, sha, &facts.default_branch).await,
        None => CommitLookup::Failed(RepoFailure {
            code: ErrorCode::ProviderError,
            why: "No GitHub transport is available to this call.".into(),
        }),
    };
    let (others, advisories, advisories_read) = if osv_complete {
        merge_advisories(owner, repo, osv, net).await?
    } else {
        (Vec::new(), End::Deferred, 0)
    };
    Ok(Github {
        read: Read {
            repository,
            repository_status,
            commit: Some(commit),
        },
        others,
        advisories,
        advisories_read,
    })
}

/// The repository's advisories merged into `osv` where they are twins of its
/// records, the ids of those that are not, how the walk ended and how many
/// advisories it read.
async fn merge_advisories(
    owner: &str,
    repo: &str,
    osv: &mut Vec<SourceResult>,
    net: &Net<'_>,
) -> Result<(Vec<String>, End, usize), ArtifactError> {
    let (rows, end, _) = advisories::github_rows(owner, repo, net, &Selection::All, None).await?;
    let known = osv
        .iter()
        .flat_map(|result| &result.advisories)
        .collect::<Vec<_>>();
    let twins = advisories::twins(&known, rows.clone());
    let twin_ids = twins.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    let others = rows
        .iter()
        .map(|row| row.id.clone())
        .filter(|id| !twin_ids.contains(id))
        .collect();
    let mut merged = SourceResult::new(Source::GitHub);
    merged.advisories = twins;
    osv.push(merged);
    Ok((others, end, rows.len()))
}

/// States what GitHub said on the row: the repository's facts and cautions, the
/// commit's, and the GitHub entry of `sources[]` with its failure, if any.
pub(super) fn apply(
    data: &mut Value,
    (owner, repo, sha): (&str, &str, &str),
    github: &Github,
    brief: &Map<String, Value>,
    view: SecurityCheckView,
) -> Result<(), ArtifactError> {
    let contract =
        security_contract().map_err(|why| ArtifactError::new(ErrorCode::ProviderError, why))?;
    let now = crate::civil_date::unix_millis();
    let today = crate::civil_date::date_millis(now);
    let read = &github.read;
    let mut fired = repository::cautions(contract, &read.repository, now, &today);
    match &read.commit {
        Some(CommitLookup::Missing) => fired.extend(repository::caution(
            contract,
            "commitNotInRepository",
            format!(
                "GitHub has no commit {} in {}, read {today}.",
                typed(sha),
                typed(&format!("{owner}/{repo}"))
            ),
        )),
        Some(CommitLookup::Found(facts)) if facts.only_in_fork_network() => {
            fired.extend(repository::caution(
                contract,
                "onlyInForkNetwork",
                format!(
                    "On no branch head, tag or pull request of {} and in the history of none of its branches or tags (the default branch and every other branch and tag were compared), read {today}.",
                    typed(&format!("{owner}/{repo}"))
                ),
            ));
        }
        _ => {}
    }
    // What GitHub holds of the commit is detail: the summary states it only
    // through a caution or an unread source, which the clause repeats.
    if view == SecurityCheckView::Details {
        let answer = data["answer"].as_str().unwrap_or_default().to_owned();
        data["answer"] = json!(format!("{answer}{}", clause(owner, repo, read)));
    }
    if let RepoRead::Found(facts) | RepoRead::Moved(facts) = &read.repository {
        data["repository"] = repository::repository_block(facts, &today);
    }
    if !fired.is_empty() {
        data["cautions"] = Value::Array(fired);
    }
    let mut source = repository::github_source(&read.repository, status(read), &today);
    let facts = read.commit.as_ref().and_then(facts_block);
    if let Some(mut facts) = facts {
        if !github.others.is_empty() {
            facts["repositoryAdvisories"] = json!(github.others);
        }
        source["facts"] = facts;
    }
    // Every GitHub read that stopped is named: a commit read left in part
    // never hides an advisory walk that stopped too.
    let listing = advisories::failure(&github.advisories)
        .filter(|_| read.commit.is_some())
        .map(|listing| RepoFailure {
            code: listing.code,
            why: advisories::unread_why(github.advisories_read, &listing, false),
        });
    let stopped = failure(read).into_iter().chain(listing).collect::<Vec<_>>();
    if let Some(first) = stopped.first() {
        // OSV's empty answer was drawn without what GitHub could not say: the
        // row is not `empty`, as a package row that was not evaluated is not.
        if let Some(row) = data.as_object_mut() {
            row.remove("status");
        }
        let whys = stopped
            .iter()
            .map(|stopped| stopped.why.clone())
            .collect::<Vec<_>>();
        source["errorCode"] = json!(first.code);
        source["why"] = json!(whys.join(" "));
        super::source::source_failed(data, &whys);
    }
    // The merge of twins listed GitHub as a source of advisories: this entry
    // replaces it, as the one GitHub entry of the row.
    let mut sources = data["sources"].as_array().cloned().unwrap_or_default();
    sources.retain(|entry| entry["source"] != "github");
    sources.push(source);
    data["sources"] = Value::Array(sources);
    if matches!(github.advisories, End::Deferred) {
        paging::push_warnings(
            data,
            [format!(
                "The repository advisories of {owner}/{repo} are compared with OSV's records once OSV's walk reaches its last page: next.nextPage reads on, and none is called \"not known to affect\" the commit before then."
            )],
        );
    }
    if !github.others.is_empty() {
        let mut row = brief.clone();
        row.insert("operation".into(), json!("repository"));
        row.insert("repo".into(), json!(format!("{owner}/{repo}")));
        data["calls"]["repositoryAdvisories"] = Continuation::related_input(
            ToolId::SecurityCheck,
            json!({ "queries": [Value::Object(row)] }),
        )
        .build();
    }
    Ok(())
}

#[cfg(test)]
#[path = "commit_tests.rs"]
mod tests;
