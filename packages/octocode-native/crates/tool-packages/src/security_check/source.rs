//! What a row asks and how it reads the answers: one file per source, named
//! here. A source receives only a typed [`Target`], so an unconfirmed package
//! name cannot reach one. While OSV is the only source, a source that fails
//! makes the row an error, so nothing about it can read as clean.
use super::Net;
use super::ecosystem::Ecosystem;
use super::osv;
use super::record::SecurityAdvisory;
use super::repository::RepoFailure;
use crate::artifact::ArtifactError;
use octocode_runtime_base::config::ResolvedConfig;
use serde::Serialize;
use serde_json::{Value, json};

/// A package the public registry confirmed, built only by the package row. A
/// row carrying `resume` continues a walk the registry confirmed earlier.
#[derive(Clone)]
pub(super) struct Confirmed {
    pub ecosystem: Ecosystem,
    pub name: String,
    /// Absent: the registry does not list the requested version, so a source
    /// hears the name alone.
    pub version: Option<String>,
    pub resume: Option<String>,
    /// A distribution release (`12`, `22.04:LTS`): OSV is asked `<ecosystem>:<release>`.
    pub release: Option<String>,
    /// A Go package path inside `name` (the module): OSV keys Go by module, so
    /// the records are narrowed to this import path afterwards.
    pub import: Option<String>,
}

/// What a source may be asked about.
pub(super) enum Target {
    Package(Confirmed),
    Advisory(String),
    Commit {
        commit: String,
        resume: Option<String>,
    },
}

/// Every source this tool has. No wildcard below: a new one does not compile
/// until each question is answered for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Source {
    Osv,
    /// A repository's own advisories: read by the rows that name a repository
    /// (`commit` with `repo`, `repository`), never through [`ask_all`].
    GitHub,
}

impl Source {
    pub(super) const ALL: [Self; 1] = [Self::Osv];

    /// The wire name in `sources[]`; core's `source` enum lists the same.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Osv => "osv",
            Self::GitHub => "github",
        }
    }

    const fn covers(self, target: &Target) -> bool {
        match (self, target) {
            (Self::Osv, Target::Package(_) | Target::Advisory(_) | Target::Commit { .. }) => true,
            (Self::GitHub, _) => false,
        }
    }

    const fn available(self, _config: &ResolvedConfig) -> bool {
        match self {
            Self::Osv | Self::GitHub => true,
        }
    }

    async fn ask(self, target: &Target, net: &Net<'_>) -> Result<SourceResult, ArtifactError> {
        match self {
            Self::Osv => osv::ask(target, net).await,
            Self::GitHub => Err(ArtifactError::new(
                octocode_contract_types::ErrorCode::ProviderError,
                "GitHub advisories are read with a repository, not asked by target",
            )),
        }
    }
}

/// Whether a source lists the package in its release, whatever the version:
/// what tells a release with no advisory at a version from a release the source
/// does not name, since both answer a version query with nothing.
pub(super) async fn names_release(
    confirmed: &Confirmed,
    net: &Net<'_>,
) -> Result<bool, ArtifactError> {
    let target = Target::Package(confirmed.clone());
    for source in Source::ALL {
        if source.covers(&target) && source.available(net.config) {
            let named = match source {
                Source::Osv => osv::names_release(confirmed, net).await?,
                Source::GitHub => false,
            };
            if named {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Every available source that covers `target`, in order.
pub(super) async fn ask_all(
    target: &Target,
    net: &Net<'_>,
) -> Result<Vec<SourceResult>, ArtifactError> {
    let mut results = Vec::new();
    for source in Source::ALL {
        if source.covers(target) && source.available(net.config) {
            results.push(source.ask(target, net).await?);
        }
    }
    Ok(results)
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum SourceStatus {
    /// It answered with findings.
    Found,
    /// A finished walk found none: none known to it, not proof of safety.
    NoneKnown,
    /// It stopped before its last page; `resume` holds what it read and reaches
    /// the rest.
    Partial,
}

/// Where a stopped source reads on (opaque to all but that source), and why it
/// stopped.
#[derive(Debug)]
pub(super) struct Resume {
    pub token: String,
    pub reason: String,
}

#[derive(Debug)]
pub(super) struct SourceResult {
    pub source: Source,
    pub advisories: Vec<SecurityAdvisory>,
    /// Records of the queried module that state import paths, none of them the
    /// package asked about: read, but not this package's.
    pub module_only: Vec<SecurityAdvisory>,
    pub resume: Option<Resume>,
    /// OSV cannot order this ecosystem's versions: the version was compared
    /// here against each record's ranges.
    pub compared_here: bool,
    /// Ids of advisories kept because their ranges (or the version) could not be
    /// compared, so they neither match nor clear the version.
    pub unmatched: Vec<String>,
}

impl SourceResult {
    pub(super) fn new(source: Source) -> Self {
        Self {
            source,
            advisories: Vec::new(),
            module_only: Vec::new(),
            resume: None,
            compared_here: false,
            unmatched: Vec::new(),
        }
    }

    pub(super) fn stopped(mut self, token: String, reason: String) -> Self {
        self.resume = Some(Resume { token, reason });
        self
    }

    fn status(&self) -> SourceStatus {
        if self.resume.is_some() {
            SourceStatus::Partial
        } else if self.advisories.is_empty() {
            SourceStatus::NoneKnown
        } else {
            SourceStatus::Found
        }
    }
}

/// How a row states its advisories.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum View {
    /// One row per advisory, with what decides an upgrade: what the summary
    /// counts and ranks.
    Brief,
    /// Each source's record as read: the `advisory` operation, and the
    /// `details` view of every row.
    Whole,
}

impl View {
    /// The advisories `view` states: `details` lists each source's record
    /// whole, with its ranges and references; the summary counts advisories,
    /// one brief row each.
    pub(super) const fn of(view: octocode_contract_types::SecurityCheckView) -> Self {
        match view {
            octocode_contract_types::SecurityCheckView::Details => Self::Whole,
            octocode_contract_types::SecurityCheckView::Summary => Self::Brief,
        }
    }
}

/// The one row builder: `sources[]`, then `advisories` only once every source
/// finished, so a missing list means not checked, and the row's status. `data`
/// holds what the row states about its target.
pub(super) fn into_row(mut data: Value, results: &[SourceResult], view: View) -> Value {
    data["sources"] = results
        .iter()
        .map(|result| json!({"source": result.source.name(), "status": result.status()}))
        .collect();
    let advisories = results
        .iter()
        .flat_map(|result| &result.advisories)
        .collect::<Vec<_>>();
    if results.iter().all(|result| result.resume.is_none()) {
        data["advisories"] = match view {
            View::Brief => {
                let mut rows = super::record::advisory_groups(&advisories)
                    .iter()
                    .map(|group| super::record::group_brief(group))
                    .collect::<Vec<_>>();
                super::record::most_severe_first(&mut rows);
                Value::Array(rows)
            }
            View::Whole => {
                let mut rows = advisories
                    .iter()
                    .filter_map(|row| serde_json::to_value(row).ok())
                    .collect::<Vec<_>>();
                super::record::drop_restated_fixes(&mut rows);
                super::record::drop_restated_text(&mut rows);
                Value::Array(rows)
            }
        };
    }
    let apart = results
        .iter()
        .flat_map(|result| &result.module_only)
        .collect::<Vec<_>>();
    if !apart.is_empty() {
        // Read, listed by id, and not this package's: the record names other
        // packages of the module (`ecosystem_specific.imports`), not this one.
        data["moduleOnly"] = json!({
            "note": "Advisories of the same module whose records list other packages and not this one; they are not counted above.",
            "advisories": super::record::advisory_groups(&apart)
                .iter()
                .map(|group| super::record::group_brief(group))
                .collect::<Vec<_>>(),
        });
    }
    // A finished check that found nothing is an answer (`affected:false`,
    // each source `noneKnown`), not an empty row: none known is what was asked.
    data
}

/// Marks `row` partial for a failed source with each of `whys` that its
/// `sources[]` entries and notices do not already state: a reason is stated
/// once, on its source or for the response ([`state_refusal`]), never again
/// in `warnings`.
pub(super) fn source_failed(row: &mut Value, whys: &[String]) {
    let notices = row
        .get(octocode_runtime_base::contracts::envelope::NOTICES)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    let stated = row
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("why").and_then(Value::as_str))
        .chain(notices)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let unstated = whys
        .iter()
        .filter(|why| {
            !stated
                .iter()
                .any(|on_source| on_source.contains(why.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    octocode_runtime_base::response::paging::mark_partial(
        row,
        octocode_runtime_base::response::paging::PartialReason::SourceFailed,
        &unstated,
    );
}

/// Records one failed read on a source's `sources[]` entry, so the entry
/// names every failure (`sourceFailed`'s meaning): a `found` or `noneKnown`
/// entry becomes `partial`, the first failure's code is kept, and each reason
/// joins `why`, never overwriting one already there.
pub(super) fn note_failure(entry: &mut Value, code: octocode_contract_types::ErrorCode, why: &str) {
    if entry["status"] == "found" || entry["status"] == "noneKnown" {
        entry["status"] = serde_json::json!("partial");
    }
    if entry.get("errorCode").is_none() {
        entry["errorCode"] = serde_json::json!(code);
    }
    let joined = match entry.get("why").and_then(Value::as_str) {
        Some(stated) if stated.contains(why) => return,
        Some(stated) => [stated, why].join(" "),
        None => why.to_owned(),
    };
    entry["why"] = serde_json::json!(joined);
}

/// Whether the rows of a response share `failure`'s words: GitHub's rate
/// limit, with its reset time and the sign-in that raises the quota, is the
/// credential's, the same on every row that reads GitHub.
pub(super) fn shared_refusal(failure: &RepoFailure) -> bool {
    failure.code == octocode_contract_types::ErrorCode::RateLimited
}

/// What a source entry says of a failed GitHub read: `unread`, what it left
/// unread (no full stop), then the failure's own words; a refusal the rows
/// share is said once for the response instead ([`state_refusal`]), and the
/// entry keeps its `errorCode`.
pub(super) fn unread_because(unread: &str, failure: &RepoFailure) -> String {
    if shared_refusal(failure) {
        format!("{unread}.")
    } else {
        format!("{unread}: {}", failure.why)
    }
}

/// States a refusal the rows share ([`shared_refusal`]) once for the
/// response: a notice, which the response stage states in the envelope's
/// warnings when two or more rows with evidence carry it, else on the first
/// row that carries it (in its hints on a failed row).
pub(super) fn state_refusal(row: &mut Value, failure: &RepoFailure) {
    if shared_refusal(failure) {
        octocode_runtime_base::response::paging::push_notice(row, failure.why.clone());
    }
}
