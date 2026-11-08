//! GitHub's global advisory database as a source of a package row: GitHub
//! decides which of its advisories affect the confirmed version, and states
//! EPSS and a numeric CVSS score beside each. Read once OSV's walk is whole. A
//! record that shares an id or alias with one OSV listed is merged into it as
//! GitHub's twin; the rest are named by id, GitHub's word and not counted, as
//! OSV stays the source the answer counts.
use super::Net;
use super::advisories::{self, End};
use super::record::{SecurityAdvisory, Selection};
use super::repository::RepoFailure;
use super::source::{Confirmed, Source, SourceResult};
use crate::contracts::security::security_contract;
use crate::response::paging::{self, PartialReason};
use octocode_contract_types::ErrorCode;
use serde_json::{Map, Value, json};

pub(super) struct Global {
    rows: Vec<SecurityAdvisory>,
    end: End,
    /// Ids GitHub matches to the version that OSV does not list.
    others: Vec<String>,
    /// Ids OSV does not list that GitHub withdrew, with when: no match.
    withdrawn: Vec<(String, String)>,
    /// What GitHub states of each record beyond its row: its withdrawal and
    /// its class when not `reviewed` ([`advisories::github_notes`]).
    notes: Vec<String>,
}

/// GitHub's advisories at the confirmed version, every page; `None` when no
/// GitHub transport exists, no version was confirmed, the row names a
/// distribution release, or GitHub keeps no database for the ecosystem.
pub(super) async fn read(confirmed: &Confirmed, net: &Net<'_>) -> Option<Global> {
    let github = net.github?;
    let version = confirmed.version.as_deref()?;
    if confirmed.release.is_some() {
        return None;
    }
    let ecosystem = security_contract()
        .ok()?
        .github_ecosystem_of_osv(&confirmed.ecosystem.to_string())?;
    let affects = format!("{}@{version}", confirmed.name);
    let affects = affects.as_str();
    let walked = advisories::walk_pages(
        |after: Option<String>| async move {
            github
                .global_advisories(ecosystem, affects, after.as_deref())
                .await
        },
        None,
    )
    .await;
    let names = [confirmed.name.clone()];
    let selection = Selection::Named(confirmed.ecosystem, &names, None);
    let mut end = walked.end;
    let mut rows = Vec::new();
    let mut notes = Vec::new();
    for advisory in &walked.advisories {
        match advisories::row(advisory, &selection) {
            Ok(row) => {
                notes.extend(advisories::github_notes(advisory));
                rows.push(row);
            }
            Err(reason) => {
                // A record that does not read stops the list: never dropped unnamed.
                end = End::Failed(RepoFailure {
                    code: ErrorCode::Decode,
                    why: format!("GitHub answered with {reason}."),
                });
                break;
            }
        }
    }
    Some(Global {
        rows,
        end,
        others: Vec::new(),
        withdrawn: Vec::new(),
        notes,
    })
}

impl Global {
    /// Merges each of GitHub's records that is the twin of an OSV record into
    /// `results`, and keeps the ids of the rest.
    pub(super) fn merge(&mut self, results: &mut Vec<SourceResult>) {
        let known = results
            .iter()
            .flat_map(|result| &result.advisories)
            .collect::<Vec<_>>();
        let twins = advisories::twins(&known, self.rows.clone());
        // A withdrawn record is no match: stated apart, never among the ids
        // GitHub matches to the version.
        let (withdrawn, others): (Vec<_>, Vec<_>) = self
            .rows
            .iter()
            .filter(|row| !twins.iter().any(|twin| twin.id == row.id))
            .partition(|row| row.withdrawn.is_some());
        self.others = others.into_iter().map(|row| row.id.clone()).collect();
        self.withdrawn = withdrawn
            .into_iter()
            .map(|row| (row.id.clone(), row.withdrawn.clone().unwrap_or_default()))
            .collect();
        if !twins.is_empty() {
            let mut merged = SourceResult::new(Source::GitHub);
            merged.advisories = twins;
            results.push(merged);
        }
    }

    /// The row's one GitHub entry states what the database said: the ids OSV
    /// does not list, each one ready `advisory` call away, or why the list is
    /// not whole.
    pub(super) fn apply(&self, data: &mut Value, brief: &Map<String, Value>) {
        let stopped = advisories::failure(&self.end);
        let status = match (&stopped, self.rows.is_empty()) {
            // Asked and nothing read: it did not answer.
            (Some(_), true) => "failed",
            (Some(_), false) => "partial",
            (None, true) => "noneKnown",
            (None, false) => "found",
        };
        if !data["sources"].is_array() {
            data["sources"] = json!([]);
        }
        let Some(sources) = data["sources"].as_array_mut() else {
            return;
        };
        let at = match sources.iter().position(|entry| entry["source"] == "github") {
            Some(at) => at,
            None => {
                sources.push(json!({"source": "github", "status": status}));
                sources.len() - 1
            }
        };
        let entry = &mut sources[at];
        if !self.others.is_empty() {
            entry["facts"]["globalAdvisories"] = json!(self.others);
        }
        let why = stopped.as_ref().map(|stopped| {
            format!(
                "GitHub's advisory database was read in part: {}",
                advisories::unread_why(self.rows.len(), stopped, false)
            )
        });
        if let (Some(stopped), Some(why)) = (&stopped, &why)
            && entry.get("errorCode").is_none()
        {
            // An entry another GitHub read wrote as found is partial now.
            if entry["status"] == "found" || entry["status"] == "noneKnown" {
                entry["status"] = json!("partial");
            }
            entry["errorCode"] = json!(stopped.code);
            entry["why"] = json!(why);
        }
        if let Some(why) = why {
            paging::mark_partial(data, PartialReason::SourceFailed, &[why]);
        }
        let mut warnings = Vec::new();
        if !self.others.is_empty() {
            let listed = self
                .others
                .iter()
                .map(|id| super::answer::stated_id(id))
                .collect::<Vec<_>>();
            warnings.push(format!(
                "GitHub's advisory database matches {} to this version and OSV does not list {}: GitHub's word, not counted in answer; related reads each.",
                listed.join(", "),
                if listed.len() == 1 { "it" } else { "them" }
            ));
        }
        warnings.extend(self.withdrawn.iter().map(|(id, at)| {
            format!(
                "GitHub withdrew {} at {}: not a match; related reads it whole.",
                super::answer::stated_id(id),
                super::answer::typed(at)
            )
        }));
        warnings.extend(self.notes.iter().cloned());
        if !warnings.is_empty() {
            paging::push_warnings(data, warnings);
        }
        let ids = self
            .others
            .iter()
            .chain(self.withdrawn.iter().map(|(id, _)| id))
            .map(String::as_str)
            .collect::<Vec<_>>();
        super::answer::advisory_calls(data, "globalAdvisories", &ids, brief);
    }
}
