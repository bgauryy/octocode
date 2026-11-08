//! What a row says first: the target checked, whether it is affected, what to
//! upgrade to, which CVEs. Everything left out of a brief row is one `advisory`
//! call away, on the row's `related` list.
use super::source::SourceResult;
use crate::id::ToolId;
use crate::response::result::Continuation;
use serde_json::{Map, Value, json};

/// What the sources' results and the target say, for the lead.
#[derive(Default)]
pub(super) struct Facts<'a> {
    pub count: usize,
    pub cves: Vec<String>,
    pub stopped: bool,
    pub version_evaluated: bool,
    /// No version was given and no registry fixes one: OSV's records span every version.
    pub all_versions: bool,
    /// A release was asked and OSV answered nothing: a release without advisories and one OSV
    /// does not name read alike.
    pub release_unmatched: bool,
    /// A source lists the package in the release asked, so nothing at the version is an answer.
    pub release_named: bool,
    /// A version of a distribution that matches per release was asked with no release, and
    /// OSV answered nothing: its answer proves nothing about any release.
    pub release_missing: bool,
    pub upgrade_to: Option<&'a str>,
    pub upgrade_note: Option<&'a str>,
    /// Runtime dependencies the target pins to one exact version.
    pub pinned_dependencies: usize,
    /// No registry reads this ecosystem: the name and version went to OSV as given.
    pub registry_unconfirmed: bool,
    /// OSV cannot order this ecosystem's versions, so the version was compared here
    /// against each record's ranges.
    pub compared_here: bool,
    /// Advisories whose ranges matched the version (all of `count` unless compared here).
    pub matched: usize,
    /// Ids of advisories listed but neither matching nor clearing the version.
    pub unmatched: Vec<String>,
}

impl Facts<'_> {
    pub(super) fn of(results: &[SourceResult]) -> Self {
        let unmatched: Vec<String> = results
            .iter()
            .flat_map(|result| result.unmatched.iter().cloned())
            .collect();
        let mut cves: Vec<String> = Vec::new();
        // CVEs of advisories that matched: an unmatched one is listed, not claimed.
        for cve in results
            .iter()
            .flat_map(|result| &result.advisories)
            .filter(|row| !unmatched.contains(&row.id))
            .flat_map(super::record::SecurityAdvisory::cves)
        {
            if !cves.iter().any(|known| known == cve) {
                cves.push(cve.to_owned());
            }
        }
        let records = results
            .iter()
            .flat_map(|result| &result.advisories)
            .collect::<Vec<_>>();
        let groups = super::record::advisory_groups(&records);
        let matched = groups
            .iter()
            .filter(|group| group.iter().any(|row| !unmatched.contains(&row.id)))
            .count();
        Self {
            // Advisories, not records: a GHSA record and its PYSEC twin are one.
            count: groups.len(),
            matched,
            compared_here: results.iter().any(|result| result.compared_here),
            registry_unconfirmed: false,
            unmatched,
            cves,
            stopped: results.iter().any(|result| result.resume.is_some()),
            version_evaluated: true,
            all_versions: false,
            release_unmatched: false,
            release_named: false,
            release_missing: false,
            upgrade_to: None,
            upgrade_note: None,
            pinned_dependencies: 0,
        }
    }
}

fn counted(count: usize) -> String {
    match count {
        1 => "1 known advisory".to_owned(),
        count => format!("{count} known advisories"),
    }
}

/// A name, id or version as the sentence states it: no character a source could
/// use to write prose into it. `answer` takes no free text from a source.
pub(super) fn typed(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "@/._:+~-".contains(*c))
        .collect()
}

/// The CVEs the sentence lists itself; past this they stay in `cves`, so one
/// package's long list does not bury what to upgrade to.
const CVES_IN_SENTENCE: usize = 5;

fn cve_clause(cves: &[String]) -> String {
    if cves.is_empty() {
        String::new()
    } else if cves.len() <= CVES_IN_SENTENCE {
        let listed = cves.iter().map(|cve| typed(cve)).collect::<Vec<_>>();
        format!(" CVEs: {}.", listed.join(", "))
    } else {
        format!(" {} CVEs, listed in cves.", cves.len())
    }
}

fn ids_clause(ids: &[String]) -> String {
    ids.iter()
        .map(|id| typed(id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a package row discloses about how its version was matched and its name read.
fn evaluation_clause(facts: &Facts<'_>) -> String {
    let mut clause = String::new();
    if facts.compared_here && !facts.stopped {
        clause.push_str(" OSV cannot order this ecosystem's versions, so each record's ranges were compared with the version here.");
        if !facts.unmatched.is_empty() && facts.matched > 0 {
            clause.push_str(&format!(
                " Listed unmatched, because their ranges could not be compared: {}.",
                ids_clause(&facts.unmatched)
            ));
        }
    }
    if facts.registry_unconfirmed && facts.release_named {
        clause.push_str(" Not registry-confirmed: no registry reads this ecosystem, so the version went to OSV as given; the name is one OSV lists in this release.");
    } else if facts.registry_unconfirmed {
        clause.push_str(" Not registry-confirmed: no registry reads this ecosystem, so the name and version went to OSV as given, and a misspelt name reads as none known.");
    }
    clause
}

const STOPPED: &str =
    "the source stopped before its last page, so this is not an answer yet: follow next.nextPage.";

/// The lead fields, in reading order: `answer`, `affected`, `upgradeTo`, `upgradeNote`, `cves`.
pub(super) fn lead(target: &str, facts: &Facts<'_>) -> Value {
    let found = facts.matched > 0;
    let target = typed(target);
    let mut sentence = match (found, facts.stopped, facts.version_evaluated) {
        (_, true, _) => format!("{target}: {STOPPED}"),
        (false, false, true) if facts.count > 0 && facts.compared_here => format!(
            "{target}: not evaluated. OSV cannot order this ecosystem's versions and {} could not be compared with this version here, so none matches or clears it; listed: {}.",
            counted(facts.count),
            ids_clause(&facts.unmatched)
        ),
        (false, false, _) if facts.release_unmatched => format!(
            "{target}: OSV has no record for this release. It answers alike for a release with no advisory and for one it does not name (forms: 12, 22.04:LTS, v3.18), so this is not none known."
        ),
        (false, false, _) if facts.release_missing => format!(
            "{target}: not evaluated. This distribution matches versions per release and none was given, so OSV's empty answer is not none known: pass release (forms: 12, 22.04:LTS, v3.18)."
        ),
        (true, false, _) if facts.all_versions => format!(
            "{target}: no version was given and no registry confirms this package, so OSV's {} span every version of it; none is a verdict on a version.",
            counted(facts.count)
        ),
        (false, false, _) if facts.all_versions => {
            format!("{target}: none known to OSV for any version.")
        }
        (true, false, true) => format!("{target} is affected: {}.", counted(facts.matched)),
        (true, false, false) => format!(
            "{target}: the requested version is not on the registry, so it was not evaluated. OSV has {} for this package at other versions.",
            counted(facts.count)
        ),
        (false, false, true) if facts.release_named => format!(
            "{target}: none known to OSV at this version. OSV answered the version with nothing, and asked by name it lists this package in the release, so that empty answer is a real one."
        ),
        (false, false, true) => format!("{target}: none known to OSV."),
        (false, false, false) => format!(
            "{target}: the requested version is not on the registry, so it was not evaluated, and OSV knows none for this package."
        ),
    };
    sentence.push_str(&cve_clause(&facts.cves));
    sentence.push_str(&evaluation_clause(facts));
    if let Some(version) = facts.upgrade_to {
        sentence.push_str(&format!(
            " Upgrade to {} to clear every one listed.",
            typed(version)
        ));
    } else if facts.upgrade_note.is_some() {
        sentence.push_str(" No upgrade is named: see upgradeNote.");
    }
    if facts.pinned_dependencies > 0 && !facts.stopped {
        sentence.push_str(&format!(
            " It pins {} to an exact version, and advisories filed against those are not in this answer: related lists a check of each.",
            match facts.pinned_dependencies {
                1 => "1 runtime dependency".to_owned(),
                count => format!("{count} runtime dependencies"),
            }
        ));
    }
    let mut data = json!({ "answer": sentence });
    if facts.version_evaluated
        && (facts.matched > 0 || facts.count == 0 || !facts.compared_here)
        && !facts.stopped
        && !facts.all_versions
        && !facts.release_unmatched
        && !facts.release_missing
    {
        data["affected"] = json!(found);
    }
    if let Some(version) = facts.upgrade_to {
        data["upgradeTo"] = json!(version);
    }
    if let Some(note) = facts.upgrade_note {
        data["upgradeNote"] = json!(note);
    }
    if !facts.cves.is_empty() {
        data["cves"] = json!(facts.cves);
    }
    data
}

/// The lead of an advisory row: each record's id, its aliases, and every
/// package it affects with each affected range and the fix closing it, from
/// the record's own events (`>=0.2.7-0 <0.2.23 (fixed 0.2.23)`). Every range
/// is listed; a git range names commits, so the record keeps it.
pub(super) fn advisory_lead(row: &Value) -> Value {
    let records = row["advisories"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let sentences = records
        .iter()
        .map(|record| {
            let mut sentence = typed(record["id"].as_str().unwrap_or_default());
            let aliases = strings(&record["aliases"]);
            if !aliases.is_empty() {
                sentence.push_str(&format!(" (aliases {})", aliases.join(", ")));
            }
            if record["withdrawn"].is_string() {
                sentence.push_str(" is withdrawn");
            }
            let packages = record["affected"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    let ranges = entry["ranges"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|range| range["type"] != "GIT")
                        .flat_map(intervals)
                        .collect::<Vec<_>>();
                    let name = typed(entry["packageName"].as_str().unwrap_or_default());
                    (!name.is_empty()).then(|| {
                        let versions = entry["versions"].as_array().map_or(0, Vec::len);
                        if !ranges.is_empty() {
                            // OSV lets `versions` add to the ranges, and osv.dev
                            // also enumerates a range's versions there: state
                            // the count, never that they are extra.
                            let listed = listed_versions(versions)
                                .map_or_else(String::new, |listed| format!(" ({listed})"));
                            return format!("{name} {}{listed}", ranges.join(", "));
                        }
                        format!("{name} ({})", unranged(entry))
                    })
                })
                .collect::<Vec<_>>();
            if packages.is_empty() {
                sentence.push('.');
            } else {
                sentence.push_str(&format!(" affects {}.", packages.join("; ")));
            }
            sentence
        })
        .collect::<Vec<_>>();
    if sentences.is_empty() {
        return json!({});
    }
    json!({ "answer": sentences.join(" ") })
}

/// What an entry with no version range holds, as the answer names it.
fn unranged(entry: &Value) -> String {
    let git = entry["ranges"]
        .as_array()
        .is_some_and(|ranges| ranges.iter().any(|range| range["type"] == "GIT"));
    let versions = entry["versions"].as_array().map_or(0, Vec::len);
    match (listed_versions(versions), git) {
        (Some(listed), _) => listed,
        (None, true) => "git commit ranges in the record".to_owned(),
        (None, false) => "no ranges or versions in the record".to_owned(),
    }
}

/// The one wording for an entry's explicit `versions` list, ranged or not;
/// `None` when it lists none.
fn listed_versions(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("the record lists 1 affected version".to_owned()),
        count => Some(format!("the record lists {count} affected versions")),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(typed)
        .collect()
}

/// The version an OSV event names (`introduced`, `fixed`, `last_affected` or
/// `limit`).
fn event_version(event: &Value) -> &str {
    ["introduced", "fixed", "last_affected", "limit"]
        .into_iter()
        .find_map(|key| event[key].as_str())
        .unwrap_or_default()
}

/// A range's events in the order to pair them. OSV only recommends a sorted
/// list, and its evaluation sorts by the range's own scheme. A `SEMVER`
/// range is sorted by SemVer (`introduced: "0"` first) when every version
/// parses; any other range (`ECOSYSTEM`: Maven, PyPI and others order
/// versions their own way) keeps the order OSV lists, as the schema
/// recommends it be sorted.
fn ordered_events(range: &Value) -> Vec<&Value> {
    let events = range["events"]
        .as_array()
        .map(|events| events.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    if range["type"] != "SEMVER" {
        return events;
    }
    let versions = events
        .iter()
        .map(|event| match event_version(event) {
            "0" => Some(None),
            version => semver::Version::parse(version.strip_prefix('v').unwrap_or(version))
                .ok()
                .map(Some),
        })
        .collect::<Option<Vec<_>>>();
    let Some(versions) = versions else {
        return events;
    };
    let mut keyed = versions.into_iter().zip(events).collect::<Vec<_>>();
    keyed.sort_by(|(left, _), (right, _)| left.cmp(right));
    keyed.into_iter().map(|(_, event)| event).collect()
}

/// A range's events as intervals, in [`ordered_events`] order: `introduced`
/// opens one (`0` or `0.0.0-0`: no lower bound), `fixed` closes it with its
/// fix, `last_affected` closes it inclusively with no fix named, and an open
/// one has no fix.
fn intervals(range: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<String> = None;
    let lower = |from: &str| match from {
        "0" | "0.0.0-0" | "0.0.0" => String::new(),
        from => format!(">={} ", typed(from)),
    };
    let events = ordered_events(range);
    for event in events {
        if let Some(from) = event["introduced"].as_str() {
            open = Some(lower(from));
        } else if let Some(fixed) = event["fixed"].as_str() {
            let fixed = typed(fixed);
            out.push(format!(
                "{}<{fixed} (fixed {fixed})",
                open.take().unwrap_or_default()
            ));
        } else if let Some(last) = event["last_affected"].as_str() {
            let last = typed(last);
            out.push(format!(
                "{}<={last} (no fix named)",
                open.take().unwrap_or_default()
            ));
        }
    }
    if let Some(from) = open {
        out.push(match from.trim() {
            "" => "every version (no fix named)".to_owned(),
            from => format!("{from} (no fix named)"),
        });
    }
    out
}

/// The lead of a commit row. An empty answer is the same for a commit OSV tracks
/// and one it has never seen, and OSV can hold a release's advisories by version
/// and none by commit: empty here proves nothing, so it never says `affected`.
pub(super) fn commit_lead(commit: &str, facts: &Facts<'_>) -> Value {
    let found = facts.count > 0;
    let commit = typed(commit);
    let mut sentence = if facts.stopped {
        format!("Commit {commit}: {STOPPED}")
    } else if found {
        format!(
            "Commit {commit} is in the affected range of {}.",
            counted(facts.count)
        )
    } else {
        format!(
            "None known to OSV for commit {commit}, which proves nothing for a commit: OSV answers the same for one it does not track."
        )
    };
    sentence.push_str(&cve_clause(&facts.cves));
    let mut data = json!({ "answer": sentence });
    if found && !facts.stopped {
        data["affected"] = json!(true);
    }
    if !facts.cves.is_empty() {
        data["cves"] = json!(facts.cves);
    }
    data
}

/// Ready calls that read every listed advisory whole: what a brief row left out
/// (details, ranges, references, CWEs) is one call away, never lost.
pub(super) fn read_whole(data: &mut Value, results: &[SourceResult], brief: &Map<String, Value>) {
    let ids = results
        .iter()
        .flat_map(|result| &result.advisories)
        .map(|row| row.id.as_str())
        .collect::<Vec<_>>();
    let per_call = crate::response::result::rows_per_call(ToolId::SecurityCheck);
    for (index, chunk) in ids.chunks(per_call).enumerate() {
        let queries = chunk
            .iter()
            .map(|id| {
                let mut row = brief.clone();
                row.insert("operation".into(), json!("advisory"));
                row.insert("id".into(), json!(id));
                Value::Object(row)
            })
            .collect::<Vec<_>>();
        data["calls"][format!("readWhole{index}")] =
            Continuation::related_input(ToolId::SecurityCheck, json!({ "queries": queries }))
                .build();
    }
}
