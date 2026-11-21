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
    /// Ids OSV does not list that GitHub withdrew, with when: never counted.
    withdrawn: Vec<(String, String)>,
    /// What GitHub states of each record beyond its row: its withdrawal and
    /// its class when not `reviewed` ([`advisories::github_notes`]).
    notes: Vec<String>,
    /// Ids GitHub classes `unreviewed`: a withdrawn one states that in its
    /// withdrawal warning, so its note is not repeated.
    unreviewed: Vec<String>,
    /// Ids GitHub classes as malware that it still matches (not withdrawn):
    /// each fires the contract's `malware` caution.
    malware: Vec<String>,
}

/// How a withdrawal warning begins, and what follows the id it names: the
/// summary view states these as one sentence ([`collapse_withdrawn`]).
const WITHDREW: &str = "GitHub withdrew ";
const WITHDREW_AT: &str = " at ";

/// The summary's one sentence for the withdrawal warnings in `warnings`: each
/// withdrawn record is retracted and never counted, so the summary names them
/// once, at the place of the first, and `related` reads them (the `details`
/// view states each with its date).
pub(super) fn collapse_withdrawn(warnings: &mut Vec<Value>) {
    let withdrawn = |warning: &Value| {
        warning
            .as_str()
            .and_then(|text| text.strip_prefix(WITHDREW))
            .and_then(|rest| rest.split_once(WITHDREW_AT))
            .map(|(id, _)| id.to_owned())
    };
    let ids = warnings.iter().filter_map(withdrawn).collect::<Vec<_>>();
    let Some(first) = warnings
        .iter()
        .position(|warning| withdrawn(warning).is_some())
    else {
        return;
    };
    let mut seen = false;
    warnings.retain(|warning| {
        let drop = withdrawn(warning).is_some();
        let keep = !drop || !seen;
        seen |= drop;
        keep
    });
    let (named, them) = match ids.as_slice() {
        [one] => (
            one.clone(),
            "so it is not counted; the related read shows it",
        ),
        [rest @ .., last] => (
            format!("{} and {last}", rest.join(", ")),
            "so they are not counted; the related reads show them",
        ),
        [] => return,
    };
    warnings[first] = json!(format!("{WITHDREW}{named}, {them}."));
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
    let mut unreviewed = Vec::new();
    let mut malware = Vec::new();
    for advisory in &walked.advisories {
        match advisories::row(advisory, &selection) {
            Ok(row) => {
                notes.extend(advisories::github_notes(advisory));
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

    /// Ids GitHub classes as malware that it still matches to the version:
    /// each a report for the row's `malware` caution ([`super::malware`]).
    pub(super) fn malware_ids(&self) -> &[String] {
        &self.malware
    }

    /// With no OSV answer to merge with: every record GitHub matches is a row
    /// of its own, and none is named as one OSV does not list. A withdrawn
    /// record is retracted: stated apart, one call away, as [`Self::merge`]
    /// states it, so neither it nor its CVE is counted.
    pub(super) fn stand_alone(&mut self, results: &mut Vec<SourceResult>) {
        self.others.clear();
        self.module_listed.clear();
        let (withdrawn, matched): (Vec<_>, Vec<_>) =
            self.rows.iter().partition(|row| row.withdrawn.is_some());
        self.withdrawn = withdrawn
            .into_iter()
            .map(|row| (row.id.clone(), row.withdrawn.clone().unwrap_or_default()))
            .collect();
        if !matched.is_empty() {
            let mut own = SourceResult::new(Source::GitHub);
            own.advisories = matched.into_iter().cloned().collect();
            results.push(own);
        }
    }

    /// The row's one GitHub entry states what the database said: the ids OSV
    /// does not list, each one ready `advisory` call away, or why the list is
    /// not whole.
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
        // A withdrawn record that is also unreviewed gets one warning naming
        // both facts, so its unreviewed note is not stated again.
        let folded = |id: &str| self.unreviewed.iter().any(|known| known == id);
        warnings.extend(self.withdrawn.iter().map(|(id, at)| {
            let both = if folded(id) {
                " and has not reviewed it (its database marks it unreviewed)"
            } else {
                ""
            };
            format!(
                "{WITHDREW}{}{WITHDREW_AT}{}{both}, so it is not counted; the related read shows it.",
                super::answer::stated_id(id),
                super::answer::typed(at)
            )
        }));
        let repeated = self
            .withdrawn
            .iter()
            .filter(|(id, _)| folded(id))
            .map(|(id, _)| advisories::unreviewed_note(&super::answer::stated_id(id)))
            .collect::<Vec<_>>();
        warnings.extend(
            self.notes
                .iter()
                .filter(|note| !repeated.contains(note))
                .cloned(),
        );
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
