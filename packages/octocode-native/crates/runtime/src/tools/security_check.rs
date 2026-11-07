//! `securityCheck`: known vulnerabilities of a package, a package URL or purl, an
//! advisory id, or a commit. Off until the user turns it on
//! (`tools.enableSecurityCheck`): admission refuses it before this runs.
mod answer;
mod osv;
mod package;
mod record;
mod source;
mod upgrade;
mod url_source;

use crate::config::ResolvedConfig;
use crate::id::ToolId;
use crate::providers::artifact::{ArtifactError, ArtifactHttp, SystemArtifactHttp};
use crate::providers::saved_answers::{ReusedSince, disclose_reuse};
use crate::response::paging::{self, PartialReason};
use crate::response::result::Continuation;
use octocode_contract_types::{ErrorCode, SecurityCheckQuery};
use octocode_io::budget::{PROVIDER_BODY_BYTES, RequestBudget};
use octocode_io::cancellation::Cancellation;
use package::Package;
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
}

/// The network a row runs on: one transport, budget and set of kept answers.
pub(super) struct Net<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    pub answers: Option<&'a dyn octocode_github::SavedAnswers>,
    pub reused: &'a ReusedSince,
    pub config: &'a ResolvedConfig,
}

pub(crate) async fn execute(
    query: SecurityCheckQuery,
    call: SecurityCall<'_>,
) -> Result<Value, ArtifactError> {
    let http = SystemArtifactHttp::shared()?.with_retries(call.max_retries);
    let budget = RequestBudget::new(
        call.deadline.min(Instant::now() + call.timeout),
        call.cancellation,
        PROVIDER_BODY_BYTES,
    );
    let reused = ReusedSince::default();
    let net = Net {
        http: &http,
        budget: &budget,
        answers: call.answers,
        reused: &reused,
        config: call.config,
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
    match query {
        SecurityCheckQuery::Package(row) => {
            package::run(
                &Package {
                    kind: row.ecosystem,
                    name: &row.package_name,
                    version: row.version.as_deref().map(String::as_str),
                    resume: row.resume.as_deref().map(String::as_str),
                    brief: brief(query),
                },
                net,
            )
            .await
        }
        SecurityCheckQuery::Url(row) => from_url(&row.url, brief(query), net).await,
        SecurityCheckQuery::Advisory(row) => advisory(&row.id, net).await,
        SecurityCheckQuery::Commit(row) => {
            commit(
                &row.commit,
                row.repo.as_deref().map(String::as_str),
                row.resume.as_deref().map(String::as_str),
                brief(query),
                net,
            )
            .await
        }
    }
}

/// A URL names a package and runs as one; one that names none ends the row
/// here, having sent nothing.
async fn from_url(
    text: &str,
    brief: Map<String, Value>,
    net: &Net<'_>,
) -> Result<Value, ArtifactError> {
    let found = url_source::parse(text).map_err(|why| {
        let mut error = ArtifactError::new(ErrorCode::InvalidInput, why.reason);
        // The repository's root tree shows the manifest that names its
        // package; GitHub already hosts the repository, and no registry or
        // OSV hears its name.
        if let Some((owner, repo)) = &why.repository {
            let lead = crate::tools::gh_shared::RepoPath::root(owner, repo)
                .lead(ToolId::GhStructure)
                .build();
            error.calls = Some(Box::new(json!({ "viewTree": lead })));
        }
        error
    })?;
    package::run(
        &Package {
            kind: found.kind,
            name: &found.name,
            version: found.version.as_deref(),
            resume: None,
            brief,
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
/// canonical spelling asked, if it differs, and the row says it was.
async fn advisory(id: &str, net: &Net<'_>) -> Result<Value, ArtifactError> {
    let mut retried = None;
    let results = match source::ask_all(&Target::Advisory(id.to_owned()), net).await {
        Err(missing) if missing.code == ErrorCode::NotFound => {
            let canonical = canonical_advisory_id(id);
            if canonical == id {
                return Err(missing);
            }
            let results = source::ask_all(&Target::Advisory(canonical.clone()), net)
                .await
                .map_err(|mut error| {
                    if error.code == ErrorCode::NotFound {
                        error.message = format!("{} Also asked as {canonical}.", missing.message);
                    }
                    error
                })?;
            retried = Some(canonical);
            results
        }
        other => other?,
    };
    let mut data = source::into_row(json!({}), &results, View::Whole);
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
    crate::response::result::finish_calls(&mut data);
    Ok(data)
}

/// One commit, asked alone. `repo` is never sent: it only checks the records
/// that came back, because OSV takes no repository.
async fn commit(
    commit: &str,
    repo: Option<&str>,
    resume: Option<&str>,
    brief: Map<String, Value>,
    net: &Net<'_>,
) -> Result<Value, ArtifactError> {
    let target = Target::Commit {
        commit: commit.to_owned(),
        resume: resume.map(str::to_owned),
    };
    let results = source::ask_all(&target, net).await?;
    let lead = answer::commit_lead(commit, &answer::Facts::of(&results));
    let mut data = source::into_row(lead, &results, View::Brief);
    answer::read_whole(&mut data, &results, &brief);
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
    Ok(data)
}

/// A warning when no returned record names `repo`: the hash matched in another
/// repository.
fn other_repository(repo: &str, results: &[source::SourceResult]) -> Option<String> {
    let plain = |url: &str| {
        url.trim_end_matches('/')
            .trim_end_matches(".git")
            .to_ascii_lowercase()
    };
    let wanted = plain(repo);
    let rows = results.iter().flat_map(|result| &result.advisories);
    let mut named = rows.clone().flat_map(record::repositories).peekable();
    named.peek()?;
    (!named.any(|url| plain(url) == wanted))
        .then(|| format!("No record names {repo}: they name the commit in another repository."))
}

#[cfg(test)]
#[path = "security_check/tests.rs"]
pub(crate) mod tests;

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
