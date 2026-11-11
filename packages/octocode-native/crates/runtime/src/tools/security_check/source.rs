//! What a row asks and how it reads the answers: one file per source, named
//! here. A source receives only a typed [`Target`], so an unconfirmed package
//! name cannot reach one. While OSV is the only source, a source that fails
//! makes the row an error, so nothing about it can read as clean.
use super::Net;
use super::ecosystem::Ecosystem;
use super::osv;
use super::record::SecurityAdvisory;
use crate::config::ResolvedConfig;
use crate::providers::artifact::ArtifactError;
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

/// `status:"empty"`: every source covering the target finished with nothing.
fn all_none_known(results: &[SourceResult]) -> bool {
    !results.is_empty()
        && results
            .iter()
            .all(|result| result.status() == SourceStatus::NoneKnown)
}

/// How a row states its advisories.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum View {
    /// What decides an upgrade, with the whole record one call away.
    Brief,
    /// The record as read: the `advisory` operation.
    Whole,
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
            View::Whole => serde_json::to_value(&advisories).unwrap_or_else(|_| json!([])),
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
    if all_none_known(results) {
        data["status"] = json!("empty");
    }
    data
}

/// Marks `row` partial for a failed source with each of `whys` that its
/// `sources[]` entries do not already state: a reason is stated once, on
/// its source, never again in `warnings`.
pub(super) fn source_failed(row: &mut Value, whys: &[String]) {
    let stated = row
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("why").and_then(Value::as_str))
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
    crate::response::paging::mark_partial(
        row,
        crate::response::paging::PartialReason::SourceFailed,
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
