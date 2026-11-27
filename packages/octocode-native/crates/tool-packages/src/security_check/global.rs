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
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::contracts::security::security_contract;
use octocode_runtime_base::response::paging;
use serde_json::{Map, Value, json};

pub(super) struct Global {
    rows: Vec<SecurityAdvisory>,
    end: End,
    /// Ids GitHub matches to the version that OSV does not list.
    others: Vec<String>,
    /// The `others` OSV lists for the package's module, in records that name
    /// other packages of it and not this one.
    module_listed: Vec<String>,
    /// Ids OSV does not list that GitHub withdrew: never counted.
    withdrawn: Vec<String>,
    /// What GitHub states of each record that its row's `githubType`
    /// cannot ([`advisories::github_notes`]), by the record's id.
    notes: Vec<(String, String)>,
    /// Ids GitHub classes `unreviewed`: a record on the row states it as
    /// `githubType`; one only GitHub matches gets a note.
    unreviewed: Vec<String>,
    /// Ids GitHub classes as malware that it still matches (not withdrawn):
    /// each fires the contract's `malware` caution.
    malware: Vec<String>,
}

/// The one sentence that names every record GitHub withdrew, in both views:
/// a withdrawn record is retracted and never counted, which bounds what the
/// answer claims; its date and text stay on its own `advisory` read.
fn withdrawn_warning(ids: &[String]) -> Option<String> {
    let named = match ids {
        [] => return None,
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    Some(format!("GitHub withdrew {named}: not counted."))
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
        .github_ecosystem_of(&confirmed.ecosystem.to_string())?;
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
    let mut unreviewed = Vec::new();
    let mut malware = Vec::new();
    for advisory in &walked.advisories {
        match advisories::row(advisory, &selection) {
            Ok(row) => {
                notes.extend(
                    advisories::github_notes(advisory)
                        .into_iter()
                        .map(|note| (advisory.ghsa_id.clone(), note)),
                );
                if advisory.kind.as_deref() == Some("unreviewed") {
                    unreviewed.push(advisory.ghsa_id.clone());
                }
                if advisory.kind.as_deref() == Some("malware") && advisory.withdrawn_at.is_none() {
                    malware.push(advisory.ghsa_id.clone());
                }
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
        module_listed: Vec::new(),
        withdrawn: Vec::new(),
        notes,
        unreviewed,
        malware,
    })
}

/// The one GitHub entry's status once both GitHub reads (the repository's own
/// advisories and the global database) have spoken: `found` when either found
/// something, `partial` when one read failed or stopped and the other
/// answered, `failed` only when neither answered, `noneKnown` only when both
/// answered empty. An entry that states no read of advisories (`notCovered`,
/// `unavailable`) takes the global read's status.
fn merged_status(existing: Option<&str>, global: &'static str) -> &'static str {
    let Some(existing @ ("found" | "noneKnown" | "partial" | "failed")) = existing else {
        return global;
    };
    match (existing, global) {
        ("found", _) | (_, "found") => "found",
        ("failed", "failed") => "failed",
        ("failed" | "partial", _) | (_, "failed" | "partial") => "partial",
        _ => "noneKnown",
    }
}

/// How much of its list GitHub's database answered for the version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Answered {
    /// Not asked, or no page read: nothing is known of its records.
    Not,
    /// Read until a page failed: its matches are a floor.
    InPart,
    /// Every page read.
    Whole,
}

impl Global {
    /// How much of the list this read reached ([`Answered`]).
    pub(super) fn answered(&self) -> Answered {
        match (advisories::failure(&self.end), self.rows.is_empty()) {
            (None, _) => Answered::Whole,
            (Some(_), true) => Answered::Not,
            (Some(_), false) => Answered::InPart,
        }
    }

    /// Merges each of GitHub's records that is the twin of an OSV record into
    /// `results`, and keeps the ids of the rest.
    pub(super) fn merge(&mut self, results: &mut Vec<SourceResult>) {
        let known = results
            .iter()
            .flat_map(|result| &result.advisories)
            .collect::<Vec<_>>();
        let twins = advisories::twins(&known, self.rows.clone());
        // A withdrawn record is retracted: stated apart, never among the ids
        // GitHub matches to the version.
        let (withdrawn, others): (Vec<_>, Vec<_>) = self
            .rows
            .iter()
            .filter(|row| !twins.iter().any(|twin| twin.id == row.id))
            .partition(|row| row.withdrawn.is_some());
        self.others = others.into_iter().map(|row| row.id.clone()).collect();
        let apart = results
            .iter()
            .flat_map(|result| &result.module_only)
            .flat_map(|row| std::iter::once(&row.id).chain(&row.aliases))
            .collect::<Vec<_>>();
        self.module_listed = self
            .rows
            .iter()
            .filter(|row| self.others.contains(&row.id))
            .filter(|row| {
                std::iter::once(&row.id)
                    .chain(&row.aliases)
                    .any(|id| apart.contains(&id))
            })
            .map(|row| row.id.clone())
            .collect();
        self.withdrawn = withdrawn.into_iter().map(|row| row.id.clone()).collect();
        if !twins.is_empty() {
            let mut merged = SourceResult::new(Source::GitHub);
            merged.advisories = twins;
            results.push(merged);
        }
    }

    /// Ids GitHub classes as malware that it still matches to the version:
    /// each a report for the row's `malware` caution ([`super::malware`]).
    pub(super) fn malware_ids(&self) -> &[String] {
        &self.malware
    }

    /// With no OSV answer to merge with: every record GitHub matches is a row
    /// of its own, and none is named as one OSV does not list. A withdrawn
    /// record is retracted: stated apart, as [`Self::merge`] states it, so
    /// neither it nor its CVE is counted.
    pub(super) fn stand_alone(&mut self, results: &mut Vec<SourceResult>) {
        self.others.clear();
        self.module_listed.clear();
        let (withdrawn, matched): (Vec<_>, Vec<_>) =
            self.rows.iter().partition(|row| row.withdrawn.is_some());
        self.withdrawn = withdrawn.into_iter().map(|row| row.id.clone()).collect();
        if !matched.is_empty() {
            let mut own = SourceResult::new(Source::GitHub);
            own.advisories = matched.into_iter().cloned().collect();
            results.push(own);
        }
    }

    /// The row's one GitHub entry states what the database said: the ids OSV
    /// does not list, each one ready `advisory` call away, or why the list is
    /// not whole. A withdrawn record is named, never read: it is not counted,
    /// and its read shows only the retraction.
    pub(super) fn apply(&self, data: &mut Value, brief: &Map<String, Value>) {
        let stopped = advisories::failure(&self.end);
        let status = match (self.answered(), self.rows.is_empty()) {
            // Asked and nothing read: it did not answer.
            (Answered::Not, _) => "failed",
            (Answered::InPart, _) => "partial",
            (Answered::Whole, true) => "noneKnown",
            (Answered::Whole, false) => "found",
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
        let merged = merged_status(entry["status"].as_str(), status);
        // A `notCovered` entry's reason is the repository's (not on GitHub):
        // once the database answered, the entry is no longer not covered,
        // and the reason is the row's, not this source's.
        let not_covered = (entry["status"] == "notCovered" && merged != "notCovered")
            .then(|| entry.as_object_mut()?.shift_remove("why"))
            .flatten()
            .and_then(|why| why.as_str().map(str::to_owned));
        entry["status"] = json!(merged);
        if !self.others.is_empty() {
            entry["facts"]["globalAdvisories"] = json!(self.others);
        }
        let why = stopped.as_ref().map(|stopped| {
            advisories::unread_why(
                "GitHub's advisory database",
                self.rows.len(),
                stopped,
                false,
            )
        });
        // An entry another GitHub read wrote keeps its reason; this one joins it.
        if let (Some(stopped), Some(why)) = (&stopped, &why) {
            super::source::note_failure(entry, stopped.code, why);
        }
        if let Some(stopped) = &stopped {
            super::source::state_refusal(data, stopped);
        }
        if let Some(why) = why {
            super::source::source_failed(data, &[why]);
        }
        let mut warnings = Vec::from_iter(not_covered);
        let (module, unlisted): (Vec<&String>, Vec<&String>) = self
            .others
            .iter()
            .partition(|id| self.module_listed.contains(id));
        let stated = |ids: &[&String]| {
            ids.iter()
                .map(|id| super::answer::stated_id(id))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let pronoun = |count: usize| if count == 1 { "it" } else { "them" };
        if !unlisted.is_empty() {
            warnings.push(format!(
                "GitHub's advisory database matches {} to this version and OSV does not list {}: GitHub's word, not counted in answer; related reads each.",
                stated(&unlisted),
                pronoun(unlisted.len())
            ));
        }
        if !module.is_empty() {
            warnings.push(format!(
                "GitHub's advisory database matches {} to this version; OSV lists {} for the module, in {} naming other packages of it and not this one: GitHub's word, not counted in answer; related reads each.",
                stated(&module),
                pronoun(module.len()),
                if module.len() == 1 { "a record" } else { "records" }
            ));
        }
        let withdrawn = self
            .withdrawn
            .iter()
            .map(|id| super::answer::stated_id(id))
            .collect::<Vec<_>>();
        warnings.extend(withdrawn_warning(&withdrawn));
        // A record on the row states its class as `githubType`; one only
        // GitHub matches is named in a warning alone, so its class is said
        // there. A withdrawn one is not counted, whatever its class.
        warnings.extend(
            self.others
                .iter()
                .filter(|id| self.unreviewed.contains(id))
                .map(|id| advisories::unreviewed_note(&super::answer::stated_id(id))),
        );
        warnings.extend(
            self.notes
                .iter()
                .filter(|(id, _)| !self.withdrawn.contains(id))
                .map(|(_, note)| note.clone()),
        );
        if !warnings.is_empty() {
            paging::push_warnings(data, warnings);
        }
        let ids = self.others.iter().map(String::as_str).collect::<Vec<_>>();
        // GitHub's records are named in warnings alone, uncounted.
        super::answer::advisory_calls(data, "globalAdvisories", &ids, brief, true);
    }
}
