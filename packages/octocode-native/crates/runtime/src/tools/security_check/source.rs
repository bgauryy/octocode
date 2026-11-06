//! What a row asks and how it reads the answers: one file per source, named
//! here. A source receives only a typed [`Target`], so an unconfirmed package
//! name cannot reach one. While OSV is the only source, a source that fails
//! makes the row an error, so nothing about it can read as clean.
use super::Net;
use super::osv;
use super::record::SecurityAdvisory;
use crate::config::ResolvedConfig;
use crate::providers::artifact::ArtifactError;
use octocode_contract_types::ArtifactType;
use serde::Serialize;
use serde_json::{Value, json};

/// A package the public registry confirmed, built only by the package row. A
/// row carrying `resume` continues a walk the registry confirmed earlier.
#[derive(Clone)]
pub(super) struct Confirmed {
    pub kind: ArtifactType,
    pub name: String,
    /// Absent: the registry does not list the requested version, so a source
    /// hears the name alone.
    pub version: Option<String>,
    pub resume: Option<String>,
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
}

impl Source {
    pub(super) const ALL: [Self; 1] = [Self::Osv];

    /// The wire name in `sources[]`; core's `source` enum lists the same.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Osv => "osv",
        }
    }

    const fn covers(self, target: &Target) -> bool {
        match (self, target) {
            (Self::Osv, Target::Package(_) | Target::Advisory(_) | Target::Commit { .. }) => true,
        }
    }

    const fn available(self, _config: &ResolvedConfig) -> bool {
        match self {
            Self::Osv => true,
        }
    }

    async fn ask(self, target: &Target, net: &Net<'_>) -> Result<SourceResult, ArtifactError> {
        match self {
            Self::Osv => osv::ask(target, net).await,
        }
    }
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
    pub resume: Option<Resume>,
}

impl SourceResult {
    pub(super) fn new(source: Source) -> Self {
        Self {
            source,
            advisories: Vec::new(),
            resume: None,
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
            View::Brief => super::record::advisory_groups(&advisories)
                .iter()
                .map(|group| super::record::group_brief(group))
                .collect(),
            View::Whole => serde_json::to_value(&advisories).unwrap_or_else(|_| json!([])),
        };
    }
    if all_none_known(results) {
        data["status"] = json!("empty");
    }
    data
}
