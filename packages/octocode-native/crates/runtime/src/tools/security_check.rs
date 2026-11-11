//! `securityCheck`: known vulnerabilities of a package, a package URL or purl, an
//! advisory id, or a commit. Off until the user turns it on
//! (`tools.enableSecurityCheck`): admission refuses it before this runs.
mod advisories;
mod answer;
mod commit;
mod dispute;
mod ecosystem;
#[cfg(test)]
pub(crate) mod github_double;
mod global;
mod malware;
mod osv;
#[cfg(test)]
pub(crate) mod osv_double;
mod package;
mod record;
mod repository;
mod source;
mod summary;
mod trust;
mod update;
mod upgrade;
mod url_source;

use crate::config::ResolvedConfig;
use crate::id::ToolId;
use crate::providers::artifact::{ArtifactError, ArtifactHttp, SystemArtifactHttp};
use crate::providers::saved_answers::{ReusedSince, disclose_reuse};
use crate::response::paging::{self, PartialReason};
use crate::response::result::Continuation;
pub(crate) use advisories::{AdvisoryFuture, AdvisoryRead, GhsaFuture, GhsaRead};
pub(crate) use commit::{CommitFacts, CommitFuture, CommitLookup, OtherRefs, Unread};
use octocode_contract_types::{ErrorCode, SecurityCheckQuery, SecurityCheckView};
use octocode_io::budget::{PROVIDER_BODY_BYTES, RequestBudget};
use octocode_io::cancellation::Cancellation;
use package::Package;
pub(crate) use repository::{
    FailedRepository, RepoFacts, RepoFailure, RepoFuture, RepoRead, RepositoryRead, Visibility,
};
use serde_json::{Map, Value, json};
use source::{Target, View};
use std::time::Instant;

/// What one call takes from the runtime besides its query.
pub(crate) struct SecurityCall<'a> {
    pub deadline: Instant,
    pub cancellation: Cancellation,
    /// The runtime's network timeout: each call's budget, within its deadline.
    pub timeout: std::time::Duration,
    /// The runtime's retries of a 5xx or 429 answer.
    pub max_retries: u8,
    /// Registry answers kept across calls; `None` keeps none.
    pub answers: Option<&'a dyn octocode_github::SavedAnswers>,
    pub config: &'a ResolvedConfig,
    /// GitHub's repository read; `None` when no transport could be built.
    pub github: Option<&'a dyn RepositoryRead>,
    /// OSV's API base (`RuntimeLimits::osv_api`).
    pub osv_api: &'a str,
}

/// The network a row runs on: one transport, budget and set of kept answers.
pub(super) struct Net<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    pub answers: Option<&'a dyn octocode_github::SavedAnswers>,
    pub reused: &'a ReusedSince,
    pub config: &'a ResolvedConfig,
    pub github: Option<&'a dyn RepositoryRead>,
    /// OSV's API base, with no trailing slash.
    pub osv_api: &'a str,
}

pub(crate) async fn execute(
    query: SecurityCheckQuery,
    call: SecurityCall<'_>,
) -> Result<Value, ArtifactError> {
    let http = SystemArtifactHttp::shared()?.with_retries(call.max_retries);
    let budget = RequestBudget::new(call.deadline, call.cancellation, PROVIDER_BODY_BYTES)
        .with_stall(call.timeout);
    let reused = ReusedSince::default();
    let net = Net {
        http: &http,
        budget: &budget,
        answers: call.answers,
        reused: &reused,
        config: call.config,
        github: call.github,
        osv_api: call.osv_api,
    };
    let mut data = run(&query, &net).await?;
    if let Some(observed) = reused.get() {
        disclose_reuse(&mut data, observed);
    }
    Ok(data)
}

/// `mainGoal` and `reasoning` of a row.
fn brief(query: &SecurityCheckQuery) -> Map<String, Value> {
    let row = serde_json::to_value(query).unwrap_or_default();
    ["mainGoal", "reasoning"]
        .into_iter()
        .filter_map(|field| Some((field.to_owned(), row.get(field)?.clone())))
        .collect()
}

async fn run(query: &SecurityCheckQuery, net: &Net<'_>) -> Result<Value, ArtifactError> {
    let mut data = operation(query, net).await?;
    mark_anonymous(&mut data, net);
    Ok(data)
}

/// Every writer of the row's `github` source shares one mark: a GitHub read
/// made without a token saw public data only, under the anonymous limit.
fn mark_anonymous(data: &mut Value, net: &Net<'_>) {
    if !net
        .github
        .is_some_and(repository::RepositoryRead::anonymous)
    {
        return;
    }
    let sources = data.get_mut("sources").and_then(Value::as_array_mut);
    for entry in sources.into_iter().flatten() {
        if entry["source"] == "github" {
            entry["anonymous"] = json!(true);
        }
    }
}

/// The row's view; the contract's default when the row states none.
fn view_of(query: &SecurityCheckQuery) -> SecurityCheckView {
    match query {
        SecurityCheckQuery::Package(row) => row.view,
        SecurityCheckQuery::Repository(row) => row.view,
        SecurityCheckQuery::Url(row) => row.view,
        SecurityCheckQuery::Advisory(row) => row.view,
        SecurityCheckQuery::Commit(row) => row.view,
    }
    .unwrap_or_default()
}

async fn operation(query: &SecurityCheckQuery, net: &Net<'_>) -> Result<Value, ArtifactError> {
    let view = view_of(query);
    match query {
        SecurityCheckQuery::Package(row) => {
            package::run(
                &Package {
                    ecosystem: row.ecosystem,
                    name: &row.package_name,
                    version: row.version.as_deref().map(String::as_str),
                    release: row.release.as_deref().map(String::as_str),
                    resume: row.resume.as_deref().map(String::as_str),
                    brief: brief(query),
                    view,
                },
                net,
            )
            .await
        }
        SecurityCheckQuery::Repository(row) => {
            repository::run(
                &row.repo,
                row.resume.as_deref().map(String::as_str),
                brief(query),
                view,
                net,
            )
            .await
        }
        SecurityCheckQuery::Url(row) => from_url(&row.url, brief(query), view, net).await,
        SecurityCheckQuery::Advisory(row) => {
            advisory(&row.id, row.repo.as_deref().map(String::as_str), view, net).await
        }
        SecurityCheckQuery::Commit(row) => {
            commit(
                &row.commit,
                row.repo.as_deref().map(String::as_str),
                row.resume.as_deref().map(String::as_str),
                brief(query),
                view,
                net,
            )
            .await
        }
    }
}

/// A URL names a package and runs as one, a GitHub repository, commit or
/// advisory and runs as that; one that names none ends the row here, having sent nothing.
async fn from_url(
    text: &str,
    brief: Map<String, Value>,
    view: SecurityCheckView,
    net: &Net<'_>,
) -> Result<Value, ArtifactError> {
    match url_source::github(text) {
        Some(Ok(url_source::GithubUrl::Advisory { id })) => {
            return advisory(&id, None, view, net).await;
        }
        Some(Ok(url_source::GithubUrl::Repository { owner, repo })) => {
            return repository::run(&format!("{owner}/{repo}"), None, brief, view, net).await;
        }
        Some(Ok(url_source::GithubUrl::Commit { owner, repo, sha })) => {
            return commit(&sha, Some(&format!("{owner}/{repo}")), None, brief, view, net).await;
        }
        Some(Err(why)) => return Err(ArtifactError::new(ErrorCode::InvalidInput, why.reason)),
        None => {}
    }
    let found = url_source::parse(text)
        .map_err(|why| ArtifactError::new(ErrorCode::InvalidInput, why.reason))?;
    package::run(
        &Package {
            ecosystem: found.ecosystem,
            name: &found.name,
            version: found.version.as_deref(),
            release: found.release.as_deref(),
            resume: None,
            brief,
            view,
        },
        net,
    )
    .await
}

/// The spelling OSV most often uses for an advisory id: the database prefix in
/// capitals (`CVE`, `PYSEC`, `GO`) and a GitHub advisory's three groups in lower
/// case (`GHSA-jfh8-c2jp-5v3q`). OSV ids are case-sensitive and a few real ones
/// keep a lower-case part (`openSUSE-SU-2019:0174-1`), so this is only the second
/// guess: [`advisory`] asks for the id as given first.
fn canonical_advisory_id(id: &str) -> String {
    match id.split_once('-') {
        Some((prefix, rest)) if prefix.eq_ignore_ascii_case("GHSA") => {
            format!("GHSA-{}", rest.to_ascii_lowercase())
        }
        Some((prefix, rest)) if prefix.chars().all(|c| c.is_ascii_alphabetic()) => {
            format!("{}-{rest}", prefix.to_ascii_uppercase())
        }
        _ => id.to_owned(),
    }
}

/// One advisory by id, asked as given. Only when OSV has no such record is the
/// canonical spelling asked, if it differs, and the row says it was. A GHSA id
/// is also read on GitHub ([`advisories::ghsa`]): its global record, and with
/// `repo` the repository's own, so an advisory OSV lacks is read whole there.
async fn advisory(
    id: &str,
    repo: Option<&str>,
    view: SecurityCheckView,
    net: &Net<'_>,
) -> Result<Value, ArtifactError> {
    let mut retried = None;
    let osv = match source::ask_all(&Target::Advisory(id.to_owned()), net).await {
        Err(missing) if missing.code == ErrorCode::NotFound => {
            let canonical = canonical_advisory_id(id);
            if canonical == id {
                Err(missing)
            } else {
                let asked = source::ask_all(&Target::Advisory(canonical.clone()), net)
                    .await
                    .map_err(|mut error| {
                        if error.code == ErrorCode::NotFound {
                            error.message =
                                format!("{} Also asked as {canonical}.", missing.message);
                        }
                        error
                    });
                if asked.is_ok() {
                    retried = Some(canonical);
                }
                asked
            }
        }
        other => other,
    };
    let github = advisories::ghsa(&canonical_advisory_id(id), repo, net).await;
    let mut osv_failed = None;
    let mut results = match osv {
        Ok(results) => results,
        // OSV has none: GitHub's record stands alone, or the id is unknown to both.
        Err(missing) if missing.code == ErrorCode::NotFound && github.found() => {
            vec![source::SourceResult::new(source::Source::Osv)]
        }
        // OSV did not answer: GitHub's record, already read whole, still stands.
        Err(failed) if github.found() => {
            osv_failed = Some(failed);
            Vec::new()
        }
        // Neither has it, or OSV failed and GitHub has none: the error names
        // what GitHub said, and each GitHub read that was not made.
        Err(mut error) => {
            if let Some(clause) = github.none_clause().or_else(|| github.failed_clause()) {
                error.message = format!("{} {clause}", error.message);
            }
            return Err(error);
        }
    };
    results.extend(github.result());
    let mut data = source::into_row(json!({}), &results, View::Whole);
    if let Some(failed) = osv_failed {
        let why = format!(
            "{}; GitHub's record stands alone. Retry later.",
            osv::unread_reason(&failed)
        );
        let entry =
            json!({"source": "osv", "status": "failed", "errorCode": failed.code, "why": why});
        match data["sources"].as_array_mut() {
            Some(sources) => sources.insert(0, entry),
            None => data["sources"] = json!([entry]),
        }
        // OSV may answer a retry, as on a package row.
        data["retryable"] = json!(true);
        // The OSV entry states why; the warnings do not repeat it.
        source::source_failed(&mut data, &[]);
    }
    github.apply(&mut data);
    // The answer leads; the whole record follows it, paged and uncapped.
    let mut lead = answer::advisory_lead(&data);
    if let (Some(lead), Some(rest)) = (lead.as_object_mut(), data.as_object()) {
        lead.extend(rest.clone());
        data = Value::Object(std::mem::take(lead));
    }
    if let Some(canonical) = retried {
        paging::push_warnings(
            &mut data,
            [format!(
                "OSV has no record {id}; this is the record it keeps as {canonical}."
            )],
        );
    }
    malware::fire(&mut data, &malware::reports(&results), false, None);
    crate::response::result::finish_calls(&mut data);
    if view == SecurityCheckView::Summary {
        // One row per advisory: OSV's record and GitHub's twin of it state
        // one claim, which `answer` gives with every range and fix.
        if data.get("advisories").is_some() {
            let records = results
                .iter()
                .flat_map(|result| &result.advisories)
                .collect::<Vec<_>>();
            let mut rows = record::advisory_groups(&records)
                .iter()
                .map(|group| record::group_summary(group))
                .collect::<Vec<_>>();
            record::most_severe_first(&mut rows);
            data["advisories"] = Value::Array(rows);
        }
        let mut query = Map::new();
        query.insert("operation".into(), json!("advisory"));
        query.insert("id".into(), json!(id));
        if let Some(repo) = repo {
            query.insert("repo".into(), json!(repo));
        }
        data = summary::advisory(data, query);
    }
    Ok(data)
}

/// One commit, asked alone. OSV takes no repository, so `repo` is never sent
/// to it: a GitHub repository is read by GitHub, and only the records that came
/// back from OSV are checked against it.
async fn commit(
    commit: &str,
    repo: Option<&str>,
    resume: Option<&str>,
    brief: Map<String, Value>,
    view: SecurityCheckView,
    net: &Net<'_>,
) -> Result<Value, ArtifactError> {
    let target = Target::Commit {
        commit: commit.to_owned(),
        resume: resume.map(str::to_owned),
    };
    let mut results = source::ask_all(&target, net).await?;
    // Only a repository GitHub hosts is read there. A resumed call repeats
    // nothing GitHub already said until OSV's walk is complete: then the row is
    // whole, with GitHub's advisories compared with all of OSV's records.
    let complete = results.iter().all(|result| result.resume.is_none());
    let mut not_github = None;
    let github = match repo.map(repository::parse) {
        Some(Ok((owner, name))) if resume.is_none() || complete => {
            let read =
                commit::read_all((&owner, &name, commit), &mut results, complete, net).await?;
            Some((owner, name, read))
        }
        Some(Err(error)) => {
            not_github = Some(format!("{} It was not read on GitHub.", error.message));
            None
        }
        _ => None,
    };
    // The lead counts after GitHub's twins merged: a withdrawal either twin
    // states is the advisory's, on a commit row as on every other.
    let lead = answer::commit_lead(commit, &answer::Facts::of(&results));
    let mut data = source::into_row(lead, &results, View::Brief);
    answer::read_whole(&mut data, &results, &brief);
    if let Some((owner, name, read)) = &github {
        commit::apply(&mut data, (owner, name, commit), read, &brief)?;
    }
    if let Some(warning) = not_github {
        paging::push_warnings(&mut data, [warning]);
    }
    if let Some(warning) = repo.and_then(|repo| other_repository(repo, &results)) {
        paging::push_warnings(&mut data, [warning]);
    }
    if let Some(resumed) = results.iter().find_map(|result| result.resume.as_ref()) {
        let mut next = brief;
        next.insert("operation".into(), json!("commit"));
        next.insert("commit".into(), json!(commit));
        if let Some(repo) = repo {
            next.insert("repo".into(), json!(repo));
        }
        summary::carry(view, &mut next);
        next.insert("resume".into(), json!(resumed.token));
        data["calls"]["nextPage"] =
            Continuation::continued(ToolId::SecurityCheck, Value::Object(next)).build();
        paging::mark_partial(
            &mut data,
            PartialReason::ProviderIncompleteResults,
            &[format!(
                "{} next.nextPage reads on from there.",
                resumed.reason
            )],
        );
    }
    crate::response::result::finish_calls(&mut data);
    let mut query = Map::new();
    query.insert("operation".into(), json!("commit"));
    query.insert("commit".into(), json!(commit));
    if let Some(repo) = repo {
        query.insert("repo".into(), json!(repo));
    }
    Ok(summary::apply(
        view,
        data,
        summary::Target {
            query,
            asked_name: None,
            asked_version: None,
            unfixed_in_major: 0,
            gives_verdict: true,
        },
    ))
}

/// A warning when no returned record names `repo`: the hash matched in another
/// repository. Repositories compare by identity, not by spelling.
fn other_repository(repo: &str, results: &[source::SourceResult]) -> Option<String> {
    let wanted = repository::RepositoryId::of(repo)?;
    let rows = results.iter().flat_map(|result| &result.advisories);
    let mut named = rows.flat_map(record::repositories).peekable();
    named.peek()?;
    (!named.any(|url| repository::RepositoryId::of(url).is_some_and(|found| found == wanted)))
        .then(|| format!("No record names {repo}: they name the commit in another repository."))
}

#[cfg(test)]
#[path = "security_check/tests.rs"]
pub(crate) mod tests;

/// This tool's recovery for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    /// A miss names each source asked (OSV, GitHub, the registry) and says
    /// that it proves nothing about any other source.
    fn error_hint(&self, code: ErrorCode, _query: &serde_json::Value) -> Option<&'static str> {
        crate::response::output::not_found_is_the_answer(code)
    }
}

#[cfg(test)]
mod advisory_id_tests {
    use super::canonical_advisory_id;

    #[test]
    fn the_canonical_spelling_is_the_usual_osv_case() {
        for (given, spelled) in [
            ("ghsa-jfh8-c2jp-5v3q", "GHSA-jfh8-c2jp-5v3q"),
            ("GHSA-JFH8-C2JP-5V3Q", "GHSA-jfh8-c2jp-5v3q"),
            ("cve-2021-44228", "CVE-2021-44228"),
            ("pysec-2020-1", "PYSEC-2020-1"),
            ("go-2020-0015", "GO-2020-0015"),
            ("RUSTSEC-2020-0071", "RUSTSEC-2020-0071"),
        ] {
            assert_eq!(canonical_advisory_id(given), spelled);
        }
    }
}
