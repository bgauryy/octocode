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
    pub upgrade_to: Option<&'a str>,
    pub upgrade_note: Option<&'a str>,
    /// Runtime dependencies the target pins to one exact version.
    pub pinned_dependencies: usize,
}

impl Facts<'_> {
    pub(super) fn of(results: &[SourceResult]) -> Self {
        let mut cves: Vec<String> = Vec::new();
        for cve in results
            .iter()
            .flat_map(|result| &result.advisories)
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
        Self {
            // Advisories, not records: a GHSA record and its PYSEC twin are one.
            count: super::record::advisory_groups(&records).len(),
            cves,
            stopped: results.iter().any(|result| result.resume.is_some()),
            version_evaluated: true,
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
fn typed(value: &str) -> String {
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

const STOPPED: &str =
    "the source stopped before its last page, so this is not an answer yet: follow next.nextPage.";

/// The lead fields, in reading order: `answer`, `affected`, `upgradeTo`, `upgradeNote`, `cves`.
pub(super) fn lead(target: &str, facts: &Facts<'_>) -> Value {
    let found = facts.count > 0;
    let target = typed(target);
    let mut sentence = match (found, facts.stopped, facts.version_evaluated) {
        (_, true, _) => format!("{target}: {STOPPED}"),
        (true, false, true) => format!("{target} is affected: {}.", counted(facts.count)),
        (true, false, false) => format!(
            "{target}: the requested version is not on the registry, so it was not evaluated. OSV has {} for this package at other versions.",
            counted(facts.count)
        ),
        (false, false, true) => format!("{target}: none known to OSV."),
        (false, false, false) => format!(
            "{target}: the requested version is not on the registry, so it was not evaluated, and OSV knows none for this package."
        ),
    };
    sentence.push_str(&cve_clause(&facts.cves));
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
    if facts.version_evaluated && !facts.stopped {
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
