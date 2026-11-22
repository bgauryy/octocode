//! `securityCheck`: known vulnerabilities of a package, a package URL or purl, an
//! advisory id, or a commit. Off until the user turns it on
//! (`tools.enableSecurityCheck`): admission refuses it before this runs.
pub mod advisories;
mod answer;
pub mod commit;
mod cvss;
mod dispute;
mod distro_order;
mod ecosystem;
#[cfg(test)]
pub(crate) mod github_double;
mod global;
mod malware;
mod osv;
#[cfg(test)]
pub(crate) mod osv_double;
mod package;
mod preview;
pub use preview::preview;
mod record;
pub mod repository;
mod source;
mod summary;
mod trust;
mod update;
mod upgrade;
mod url_source;

use crate::artifact::{ArtifactError, ArtifactHttp, SystemArtifactHttp};
pub use advisories::{AdvisoryFuture, AdvisoryRead, GhsaFuture, GhsaRead};
pub use commit::{CommitFacts, CommitFuture, CommitLookup, OtherRefs, Unread};
use octocode_contract_types::{ErrorCode, SecurityCheckQuery, SecurityCheckView};
use octocode_io::budget::{PROVIDER_BODY_BYTES, RequestBudget};
use octocode_io::cancellation::Cancellation;
use octocode_runtime_base::config::ResolvedConfig;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::providers::saved_answers::{ReusedSince, disclose_reuse};
use octocode_runtime_base::response::paging::{self, PartialReason};
use octocode_runtime_base::response::result::Continuation;
use package::Package;
pub use repository::{
    FailedRepository, RepoFacts, RepoFailure, RepoFuture, RepoRead, RepositoryRead, Visibility,
};
use serde_json::{Map, Value, json};
use source::{Target, View};
use std::time::Instant;

/// What one call takes from the runtime besides its query.
pub struct SecurityCall<'a> {
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
    /// What the rows of this call share ([`crate::artifact::CallShare`]); `None`: the
    /// row shares with no other.
    pub share: Option<&'a crate::artifact::CallShare>,
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
    /// What the row shares with the other rows of its call: reads made once,
    /// and one bound on OSV requests in flight.
    pub share: &'a crate::artifact::CallShare,
}

pub async fn execute(
    query: SecurityCheckQuery,
    call: SecurityCall<'_>,
) -> Result<Value, ArtifactError> {
    let http = SystemArtifactHttp::shared().with_retries(call.max_retries);
    let budget = RequestBudget::new(call.deadline, call.cancellation, PROVIDER_BODY_BYTES)
        .with_stall(call.timeout);
    let reused = ReusedSince::default();
    let alone;
    let share = match call.share {
        Some(share) => share,
        None => {
            alone = crate::artifact::CallShare::new();
            &alone
        }
    };
    let net = Net {
        http: &http,
        budget: &budget,
        answers: call.answers,
        reused: &reused,
        config: call.config,
        github: call.github,
        osv_api: call.osv_api,
        share,
    };
    let mut data = run(&query, &net).await?;
    if let Some(observed) = reused.get() {
        disclose_reuse(&mut data, observed);
    }
    Ok(data)
}

/// The brief of a row ([`octocode_runtime_base::contracts::BRIEF_FIELDS`]).
fn brief(query: &SecurityCheckQuery) -> Map<String, Value> {
    let row = serde_json::to_value(query).unwrap_or_default();
    octocode_runtime_base::contracts::BRIEF_FIELDS
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
                    version: crate::artifact::query::trimmed_version(
                        row.version.as_deref().map(String::as_str),
                    ),
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
                (&row.owner, &row.repo),
                row.resume.as_deref().map(String::as_str),
                brief(query),
                view,
                net,
            )
            .await
        }
        SecurityCheckQuery::Url(row) => from_url(&row.url, brief(query), view, net).await,
        SecurityCheckQuery::Advisory(row) => {
            advisory(&row.id, named(&row.owner, &row.repo), view, net).await
        }
        SecurityCheckQuery::Commit(row) => {
            commit(
                &row.ref_,
                named(&row.owner, &row.repo),
                row.resume.as_deref().map(String::as_str),
                brief(query),
                view,
                net,
            )
            .await
        }
    }
}

/// The repository a row names: `owner` + `repo`, which the contract takes
/// both or neither of.
fn named<'a, T: std::ops::Deref<Target = String>>(
    owner: &'a Option<T>,
    repo: &'a Option<T>,
) -> Option<(&'a str, &'a str)> {
    Some((owner.as_deref()?.as_str(), repo.as_deref()?.as_str()))
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
            return repository::run((&owner, &repo), None, brief, view, net).await;
        }
        Some(Ok(url_source::GithubUrl::Commit { owner, repo, sha })) => {
            return commit(&sha, Some((&owner, &repo)), None, brief, view, net).await;
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
    repo: Option<(&str, &str)>,
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
    let aliases = follow_aliases(&mut results, net).await;
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
    aliases.apply(&mut data, view);
    malware::fire(&mut data, &malware::reports(&results), false, None);
    octocode_runtime_base::response::result::finish_calls(&mut data);
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
        data = summary::advisory(data);
    } else if let Some(rows) = data.get_mut("advisories").and_then(Value::as_array_mut) {
        record::drop_restated_fixes(rows);
        record::drop_derivable_links(rows);
    }
    Ok(data)
}

/// The aliases an advisory row read because a record it holds names no
/// package, and those OSV did not answer, with why.
#[derive(Default)]
struct Aliases {
    /// `(record, alias)`: the alias's records are listed after the record's.
    read: Vec<(String, String)>,
    /// `(record, alias, why)`.
    unread: Vec<(String, String, String)>,
}

/// A record whose `affected` entries name no package (OSV's CVE records: no
/// entry, or only a GIT range) states no package or fix; the records of its
/// aliases, the same advisory in another database, do. Each alias no record
/// of the row already is gets read from OSV, all at once, and its records are
/// listed with the source that answered. A related record is another
/// advisory and is not read, and an alias the row holds (GitHub's record of
/// it) is not read again.
async fn follow_aliases(results: &mut Vec<source::SourceResult>, net: &Net<'_>) -> Aliases {
    let held = results
        .iter()
        .flat_map(|result| &result.advisories)
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let mut wanted: Vec<(String, String)> = Vec::new();
    // OSV's records only: GitHub's record of an alias is already that
    // alias's own, and is held.
    let osv = results
        .iter()
        .filter(|result| result.source == source::Source::Osv);
    for record in osv.flat_map(|result| &result.advisories) {
        let names_package = record
            .affected
            .iter()
            .any(|entry| entry.get("packageName").is_some() || entry.get("purl").is_some());
        if names_package {
            continue;
        }
        for alias in &record.aliases {
            if !held.contains(alias) && !wanted.iter().any(|(_, known)| known == alias) {
                wanted.push((record.id.clone(), alias.clone()));
            }
        }
    }
    let targets = wanted
        .iter()
        .map(|(_, alias)| Target::Advisory(alias.clone()))
        .collect::<Vec<_>>();
    let reads =
        futures_util::future::join_all(targets.iter().map(|target| source::ask_all(target, net)))
            .await;
    let mut aliases = Aliases::default();
    for ((record, alias), read) in wanted.into_iter().zip(reads) {
        match read {
            Ok(found) => {
                for mut answered in found {
                    // A record the row holds already is not listed twice.
                    answered
                        .advisories
                        .retain(|record| !held.contains(&record.id));
                    match results
                        .iter_mut()
                        .find(|result| result.source == answered.source)
                    {
                        Some(result) => result.advisories.extend(answered.advisories),
                        None => results.push(answered),
                    }
                }
                aliases.read.push((record, alias));
            }
            Err(error) => aliases.unread.push((
                record,
                alias,
                octocode_runtime_base::security::quoted(&error.message).into_owned(),
            )),
        }
    }
    aliases
}

impl Aliases {
    /// Says which records were read for an alias, and leads to each alias
    /// that was not. The details view lists an alias's records after the
    /// record; the summary groups them into its row, so it says the answer
    /// states them and the details view lists them. An unread alias is a
    /// record the row does not list: its read is a related call, which the
    /// summary keeps ([`summary::advisory`]).
    fn apply(&self, data: &mut Value, view: SecurityCheckView) {
        let read = self.read.iter().map(|(record, alias)| match view {
            SecurityCheckView::Details => format!(
                "{record} names no package; {alias}, its alias, is listed after it for the packages and fixes."
            ),
            SecurityCheckView::Summary => format!(
                "{record} names no package; the answer states the packages and fixes of {alias}, its alias, and view details lists its record."
            ),
        });
        let unread = self.unread.iter().map(|(record, alias, why)| {
            format!("{record} names no package, and {alias}, its alias, could not be read ({why}); related reads it.")
        });
        paging::push_warnings(data, read.chain(unread).collect::<Vec<_>>());
        if self.unread.is_empty() {
            return;
        }
        let queries = self
            .unread
            .iter()
            .map(|(_, alias, _)| json!({"operation": "advisory", "id": alias}))
            .collect::<Vec<_>>();
        data["calls"]["readAlias"] =
            Continuation::related_input(ToolId::SecurityCheck, json!({ "queries": queries }))
                // The alias's record is on no row.
                .required()
                .build();
    }
}

/// One commit, asked alone. OSV takes no repository, so `repo` is never sent
/// to it: a GitHub repository is read by GitHub, and only the records that came
/// back from OSV are checked against it.
async fn commit(
    commit: &str,
    repo: Option<(&str, &str)>,
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
    let github = match repo {
        Some((owner, name)) if resume.is_none() || complete => {
            let read = commit::read_all((owner, name, commit), &mut results, complete, net).await?;
            Some((owner, name, read))
        }
        _ => None,
    };
    // The lead counts after GitHub's twins merged: a withdrawal either twin
    // states is the advisory's, on a commit row as on every other.
    let lead = answer::commit_lead(commit, &answer::Facts::of(&results));
    let mut data = source::into_row(lead, &results, View::of(view));
    if let Some((owner, name, read)) = &github {
        commit::apply(&mut data, (owner, name, commit), read, &brief, view)?;
    }
    if let Some(warning) = repo.and_then(|repo| other_repository(repo, &results)) {
        paging::push_warnings(&mut data, [warning]);
    }
    if let Some(resumed) = results.iter().find_map(|result| result.resume.as_ref()) {
        let mut next = brief;
        next.insert("operation".into(), json!("commit"));
        next.insert("ref".into(), json!(commit));
        if let Some((owner, name)) = repo {
            repository::name_in(&mut next, owner, name);
        }
        summary::carry(view, &mut next);
        next.insert("resume".into(), json!(resumed.token));
        octocode_runtime_base::response::collection::insert_row_walk(
            &mut data,
            ToolId::SecurityCheck,
            Continuation::continued(ToolId::SecurityCheck, Value::Object(next)),
        );
        paging::mark_partial(
            &mut data,
            PartialReason::ProviderIncompleteResults,
            &[format!(
                "{} The reply's next reads on from there.",
                resumed.reason
            )],
        );
    }
    octocode_runtime_base::response::result::finish_calls(&mut data);
    Ok(summary::apply(
        view,
        data,
        summary::Target {
            asked_name: None,
            asked_version: None,
            unfixed_in_major: 0,
            gives_verdict: true,
            upgrade_prerelease: false,
        },
    ))
}

/// A warning when no returned record names `repo`: the hash matched in another
/// repository. Repositories compare by identity, not by spelling.
fn other_repository(
    (owner, name): (&str, &str),
    results: &[source::SourceResult],
) -> Option<String> {
    let repo = format!("{owner}/{name}");
    let wanted = repository::RepositoryId::of(&repo)?;
    let rows = results.iter().flat_map(|result| &result.advisories);
    let mut named = rows.flat_map(record::repositories).peekable();
    named.peek()?;
    (!named.any(|url| repository::RepositoryId::of(url).is_some_and(|found| found == wanted)))
        .then(|| format!("No record names {repo}: they name the commit in another repository."))
}

#[cfg(test)]
#[path = "security_check/tests.rs"]
pub(crate) mod tests;

/// This tool's recovery for the shared response stages: a target its
/// registry or database does not know is a spelling to check there, never a
/// path to list; every other code takes the shared table's.
pub struct Output;
impl octocode_runtime_base::response::output::ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        (code == ErrorCode::NotFound).then_some(NOT_FOUND_HINT)
    }
}

/// The recovery of a package, advisory id or repository not found.
const NOT_FOUND_HINT: &str = "Check the name, id or repository as its registry or database writes it (the ecosystem's package name, a GHSA, CVE or OSV id, owner and repo of a GitHub repository), then retry with that spelling.";

#[cfg(test)]
#[path = "security_check/advisory_id_tests.rs"]
mod advisory_id_tests;
