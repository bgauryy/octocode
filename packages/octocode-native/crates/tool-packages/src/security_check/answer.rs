//! What a row says first: the target checked, whether it is affected, what to
//! upgrade to, which CVEs. Everything left out of a brief row is one `advisory`
//! call away, on the row's `related` list.
use super::source::SourceResult;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
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
    /// A version the registry does not hold (unpublished, removed for
    /// malware) and the advisories whose records list it by name: they do
    /// affect it, whatever the registry says.
    pub unregistered: Option<(&'a str, Vec<String>)>,
    /// OSV, asked at the `unregistered` version, lists none of its records at it.
    pub unregistered_clear: bool,
    /// The registry (`artifactSearch` ecosystem) that does not list the name:
    /// it went to OSV as given.
    pub unregistered_name: Option<&'static str>,
    /// No registry confirmed the name: the name and version went to OSV as given.
    pub registry_unconfirmed: bool,
    /// OSV cannot order this ecosystem's versions, so the version was compared here
    /// against each record's ranges.
    pub compared_here: bool,
    /// Records that state a withdrawal, each id once with when: stated apart,
    /// never counted, and never hiding a live record of the same advisory
    /// ([`super::record::Standing`]).
    pub withdrawn: Vec<(String, String)>,
    /// Advisories whose ranges matched the version (all of `count` unless compared here).
    pub matched: usize,
    /// Ids of advisories listed but neither matching nor clearing the version.
    pub unmatched: Vec<String>,
    /// Other databases' records of a listed advisory that do not list the version.
    pub disputed: Vec<String>,
    /// The Go module a package path was read in: OSV keys Go by module, so its
    /// records name the module's other packages too.
    pub module: Option<&'a str>,
    /// One id for each advisory OSV lists for the module whose records name
    /// other packages and not this one: set apart, never counted.
    pub module_only: Vec<String>,
}

impl Facts<'_> {
    pub(super) fn of(results: &[SourceResult]) -> Self {
        let unmatched: Vec<String> = results
            .iter()
            .flat_map(|result| result.unmatched.iter().cloned())
            .collect();
        let records = results
            .iter()
            .flat_map(|result| &result.advisories)
            .collect::<Vec<_>>();
        let groups = super::record::advisory_groups(&records);
        // A withdrawn record is stated apart and never counted, on every row
        // type alike ([`super::record::Standing`]).
        let standings = groups
            .iter()
            .map(|group| super::record::Standing::of(group))
            .collect::<Vec<_>>();
        let withdrawn = standings
            .iter()
            .flat_map(|standing| &standing.withdrawn)
            .map(|(id, at)| ((*id).to_owned(), (*at).to_owned()))
            .collect();
        let live = standings
            .iter()
            .filter(|standing| !standing.is_withdrawn())
            .map(|standing| &standing.live)
            .collect::<Vec<_>>();
        let mut cves: Vec<String> = Vec::new();
        // CVEs of advisories that matched: an unmatched one is listed, not claimed.
        for cve in live
            .iter()
            .flat_map(|records| records.iter())
            .filter(|row| !unmatched.contains(&row.id))
            .flat_map(|row| row.cves())
        {
            if !cves.iter().any(|known| known == cve) {
                cves.push(cve.to_owned());
            }
        }
        let apart = results
            .iter()
            .flat_map(|result| &result.module_only)
            .collect::<Vec<_>>();
        let module_only = super::record::advisory_groups(&apart)
            .iter()
            .filter_map(|group| group.first().map(|row| row.id.clone()))
            .collect();
        let mut disputed: Vec<String> = Vec::new();
        for id in records.iter().flat_map(|row| &row.disputed_by) {
            if !disputed.contains(id) {
                disputed.push(id.clone());
            }
        }
        let matched = live
            .iter()
            .filter(|records| records.iter().any(|row| !unmatched.contains(&row.id)))
            .count();
        Self {
            // Advisories, not records: a GHSA record and its PYSEC twin are one.
            count: live.len(),
            withdrawn,
            matched,
            compared_here: results.iter().any(|result| result.compared_here),
            registry_unconfirmed: false,
            unmatched,
            disputed,
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
            unregistered: None,
            unregistered_clear: false,
            unregistered_name: None,
            module: None,
            module_only,
        }
    }
}

impl Facts<'_> {
    /// The row's `affected`: whether the version is affected, when the
    /// sources' answer decides it. A version the registry does not hold is
    /// affected by a record that lists it by name; an evaluated version is
    /// affected by a match, and clear when its walk finished with none.
    pub(super) fn verdict(&self) -> Option<bool> {
        if self.version_evaluated
            && (self.matched > 0 || self.count == 0 || !self.compared_here)
            && !self.stopped
            && !self.all_versions
            && !self.release_unmatched
            && !self.release_missing
        {
            return Some(self.matched > 0);
        }
        (self
            .unregistered
            .as_ref()
            .is_some_and(|(_, listed)| !listed.is_empty())
            && !self.stopped)
            .then_some(true)
    }
}

/// `count` known advisories, in the right number.
fn counted(count: usize) -> String {
    octocode_engine::text::counted(count, "known advisory", "known advisories")
}

/// A name, id or version as the sentence states it: no character a source could
/// use to write prose into it. `answer` takes no free text from a source.
pub(super) fn typed(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "@/._:+~-".contains(*c))
        .collect()
}

/// An id (an advisory's, an alias, a CVE) as a sentence states it: the id
/// itself when the published contract takes it as an `advisory` id, otherwise
/// words that say so. An id comes from a source, so it is data: filtering its
/// characters would run prose together into an id that does not exist.
pub(super) fn stated_id(id: &str) -> String {
    let accepted = octocode_runtime_base::contracts::validate(
        ToolId::SecurityCheck.as_str(),
        json!({ "queries": [{ "operation": "advisory", "id": id }] }),
    )
    .is_ok();
    if accepted {
        id.to_owned()
    } else {
        "an id the contract refuses".to_owned()
    }
}

/// The CVEs the sentence lists itself; past this they stay in `cves`, so one
/// package's long list does not bury what to upgrade to.
const CVES_IN_SENTENCE: usize = 5;

fn cve_clause(cves: &[String]) -> String {
    if cves.is_empty() {
        String::new()
    } else if cves.len() <= CVES_IN_SENTENCE {
        let listed = cves.iter().map(|cve| stated_id(cve)).collect::<Vec<_>>();
        format!(" CVEs: {}.", listed.join(", "))
    } else {
        format!(" {} CVEs, listed on each advisory.", cves.len())
    }
}

fn ids_clause(ids: &[String]) -> String {
    ids.iter()
        .map(|id| stated_id(id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The advisories OSV lists for the package's module whose records name other
/// packages of it: not counted, and one read away (`moduleOnly` of the
/// `details` view).
fn module_clause(facts: &Facts<'_>) -> String {
    let (Some(module), false) = (facts.module, facts.module_only.is_empty()) else {
        return String::new();
    };
    format!(
        " OSV also lists {} for the module {} whose records name other packages of it and not this one ({}): they are not counted here; the details view's moduleOnly lists them.",
        octocode_engine::text::counted(facts.module_only.len(), "advisory", "advisories"),
        typed(module),
        ids_clause(&facts.module_only)
    )
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
    clause
}

/// The warning of a row no registry confirmed: a public registry that does
/// not list the name (removed, as for malware) says so on any row. Where no
/// registry reads the ecosystem, it is said only beside a verdict of none
/// known (`affected:false`), which a misspelt name would fake; a row with
/// advisories, or with no verdict, claims nothing a misspelling could fake,
/// and `registryConfirmed:false` says the rest. It names no package, so the
/// rows of one call that share it state it once.
pub(super) fn unconfirmed_warning(facts: &Facts<'_>) -> Option<String> {
    if !facts.registry_unconfirmed {
        return None;
    }
    if let Some(registry) = facts.unregistered_name {
        return Some(format!(
            "Not registry-confirmed: the public {registry} registry does not list this name (removed, as for malware, or never published), so the name and version went to OSV as given."
        ));
    }
    if facts.verdict() != Some(false) {
        return None;
    }
    Some(if facts.release_named {
        "Not registry-confirmed: no registry reads this ecosystem, so the version went to OSV as given; the name is one OSV lists in this release.".to_owned()
    } else {
        "Not registry-confirmed: no registry reads this ecosystem, so the name and version went to OSV as given, and a misspelt name reads as none known.".to_owned()
    })
}

const STOPPED: &str = "the source stopped before its last page, so this is not an answer yet: follow the reply's next.";

/// One clause of a package row's `answer`, and the field of a summary row
/// that states the same, if one does.
struct Clause {
    text: String,
    stated_by: Option<&'static str>,
}

/// A package row's `answer`, clause by clause in sentence order: the details
/// view states the whole sentence, the summary only the clauses no field of
/// its row states.
#[derive(Default)]
pub(super) struct Answer(Vec<Clause>);

impl Answer {
    /// An answer of one sentence no field states (a rewrite of the lead).
    pub(super) fn whole(text: &str) -> Self {
        let mut answer = Self::default();
        answer.push(text.to_owned(), None);
        answer
    }

    fn push(&mut self, text: String, stated_by: Option<&'static str>) {
        if !text.is_empty() {
            self.0.push(Clause { text, stated_by });
        }
    }

    /// The whole sentence.
    fn sentence(&self) -> String {
        self.0.iter().map(|clause| clause.text.as_str()).collect()
    }

    /// What `row` (a summary row) states in no field: the clauses whose
    /// field it does not hold; `None` when its fields state every one.
    pub(super) fn unstated(&self, row: &Map<String, Value>) -> Option<String> {
        let text = self
            .0
            .iter()
            .filter(|clause| {
                clause
                    .stated_by
                    .is_none_or(|field| !row.contains_key(field))
            })
            .map(|clause| clause.text.as_str())
            .collect::<String>();
        let text = text.trim_start();
        (!text.is_empty()).then(|| text.to_owned())
    }
}

/// The lead fields, in reading order: `answer`, `affected`, `upgradeTo`,
/// `upgradeNote`, `cves`; and the answer clause by clause.
pub(super) fn lead(target: &str, facts: &Facts<'_>) -> (Value, Answer) {
    let found = facts.matched > 0;
    let target = typed(target);
    // A plain verdict (affected, with its count; none known, which proves
    // only that) is what `affected` and `advisoryCount` state; every other
    // opening says what no field does.
    let (opening, verdict) = match (found, facts.stopped, facts.version_evaluated) {
        (_, true, _) => (format!("{target}: {STOPPED}"), None),
        (false, false, true) if facts.count > 0 && facts.compared_here => (
            format!(
                "{target}: not evaluated. OSV cannot order this ecosystem's versions and {} could not be compared with this version here, so none matches or clears it; listed: {}.",
                counted(facts.count),
                ids_clause(&facts.unmatched)
            ),
            None,
        ),
        (false, false, _) if facts.release_unmatched => (
            format!(
                "{target}: OSV has no record for this release. It answers alike for a release with no advisory and for one it does not name (forms: 12, 22.04:LTS, v3.18), so this is not none known."
            ),
            None,
        ),
        (false, false, _) if facts.release_missing => (
            format!(
                "{target}: not evaluated. This distribution matches versions per release and none was given, so OSV's empty answer is not none known: pass release (forms: 12, 22.04:LTS, v3.18)."
            ),
            None,
        ),
        (true, false, _) if facts.all_versions => (
            format!(
                "{target}: no version was given and no registry confirms this package, so OSV's {} span every version of it; none is a verdict on a version.",
                counted(facts.count)
            ),
            None,
        ),
        (false, false, _) if facts.all_versions => (
            format!("{target}: none known to OSV for any version."),
            None,
        ),
        (true, false, true) => (
            format!("{target} is affected: {}.", counted(facts.matched)),
            Some("affected"),
        ),
        (true, false, false) => (
            match &facts.unregistered {
                Some((version, listed)) if !listed.is_empty() => format!(
                    "{target}: version {} is not on the registry (yanked, unpublished or removed), and OSV lists it as affected by {}.",
                    typed(version),
                    ids_clause(listed)
                ),
                Some((version, _)) if facts.unregistered_clear => format!(
                    "{target}: version {} is not on the registry (yanked, unpublished or removed). OSV has {} for this package, and asked at this version it lists none of them as affecting it.",
                    typed(version),
                    counted(facts.count)
                ),
                _ => format!(
                    "{target}: the requested version is not on the registry, so it was not evaluated. OSV has {} for this package, none listing this version by name (their ranges were not compared).",
                    counted(facts.count)
                ),
            },
            None,
        ),
        (false, false, true) if facts.release_named => (
            format!(
                "{target}: none known to OSV at this version. OSV answered the version with nothing, and asked by name it lists this package in the release, so that empty answer is a real one."
            ),
            None,
        ),
        (false, false, true) => (
            format!("{target}: none known to OSV, which does not prove it safe."),
            Some("affected"),
        ),
        (false, false, false) => (
            format!(
                "{target}: the requested version is not on the registry, so it was not evaluated, and OSV knows none for this package."
            ),
            None,
        ),
    };
    let mut answer = Answer::default();
    answer.push(opening, verdict);
    // Each advisory row states its CVEs.
    answer.push(cve_clause(&facts.cves), Some("advisories"));
    // The count leaves withdrawn records out; the details name them.
    answer.push(withdrawn_clause(&facts.withdrawn), Some("advisoryCount"));
    if found && !facts.stopped && !facts.disputed.is_empty() {
        let (noun, verb) = if facts.disputed.len() == 1 {
            ("a record", "does")
        } else {
            ("records", "do")
        };
        answer.push(
            format!(
                " Disputed by {}, {noun} of a listed advisory in another database that {verb} not list this version; see warnings.",
                ids_clause(&facts.disputed)
            ),
            Some("warnings"),
        );
    }
    answer.push(evaluation_clause(facts), None);
    answer.push(module_clause(facts), Some("moduleOnly"));
    if let Some(version) = facts.upgrade_to {
        answer.push(
            format!(" Upgrade to {} to clear every one listed.", typed(version)),
            Some("upgradeTo"),
        );
    } else if facts.upgrade_note.is_some() {
        answer.push(
            " No upgrade is named: see upgradeNote.".to_owned(),
            Some("upgradeNote"),
        );
    }
    if facts.pinned_dependencies > 0 && !facts.stopped {
        answer.push(
            format!(
                " It pins {} to an exact version, and advisories filed against those are not in this answer: related lists a check of each.",
                match facts.pinned_dependencies {
                    1 => "1 runtime dependency".to_owned(),
                    count => format!("{count} runtime dependencies"),
                }
            ),
            None,
        );
    }
    let mut data = json!({ "answer": answer.sentence() });
    if let Some(affected) = facts.verdict() {
        data["affected"] = json!(affected);
    }
    if let Some(version) = facts.upgrade_to {
        data["upgradeTo"] = json!(version);
    }
    if let Some(note) = facts.upgrade_note {
        data["upgradeNote"] = json!(note);
    }
    // CVEs are stated on each advisory; the answer names them first.
    (data, answer)
}

/// The lead of an advisory row: each record's id, its aliases, and every
/// package it affects with each affected range and the fix closing it, from
/// the record's own events (`>=0.2.7-0 <0.2.23 (fixed 0.2.23)`). Every range
/// is listed; a git range names commits, so the record keeps it. Records of
/// one advisory under its aliases (GitHub's and PyPI's of one CVE) that
/// state the same ranges are one clause naming each; records whose ranges
/// differ keep a clause each.
pub(super) fn advisory_lead(row: &Value) -> Value {
    let records = row["advisories"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    // OSV and GitHub each carry a record for one id: when both state the
    // same claim (the same packages, ranges and fixes), the first states it.
    let mut stated = std::collections::HashSet::new();
    let mut clauses: Vec<Vec<RecordClause>> = Vec::new();
    for clause in records.iter().map(RecordClause::of) {
        if !stated.insert(clause.claim()) {
            continue;
        }
        match clauses.iter_mut().find(|group| {
            group[0].states_as(&clause) && group.iter().any(|known| known.aliased(&clause))
        }) {
            Some(group) => group.push(clause),
            None => clauses.push(vec![clause]),
        }
    }
    if clauses.is_empty() {
        return json!({});
    }
    let sentences = clauses
        .iter()
        .map(|group| RecordClause::sentence(group))
        .collect::<Vec<_>>();
    json!({ "answer": sentences.join(" ") })
}

/// What one record of an advisory row states, as its clause says it.
struct RecordClause {
    /// The record's id and aliases, as [`stated_id`] states them.
    id: String,
    aliases: Vec<String>,
    /// When it was withdrawn: what it stated follows in the past tense.
    withdrawn: Option<String>,
    /// Each package it names: as the sentence shows it, and its claim as
    /// both sources make it alike ([`claimed`]).
    packages: Vec<(String, String)>,
    /// The repositories its commit ranges are in, when it names no package.
    repositories: Vec<String>,
}

impl RecordClause {
    fn of(record: &Value) -> Self {
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
                    let claim_of = |range: &str| format!("{name} {range}");
                    let versions = entry["versions"].as_array().map_or(0, Vec::len);
                    if !ranges.is_empty() {
                        // OSV lets `versions` add to the ranges, and osv.dev
                        // also enumerates a range's versions there: state
                        // the count, never that they are extra.
                        let listed = listed_versions(versions)
                            .map_or_else(String::new, |listed| format!(" ({listed})"));
                        let claim = claim_of(
                            &ranges
                                .iter()
                                .map(|range| claimed(range))
                                .collect::<Vec<_>>()
                                .join(", "),
                        );
                        return (format!("{name} {}{listed}", ranges.join(", ")), claim);
                    }
                    if let Some(written) = written_range(entry) {
                        return (
                            format!("{name} {}", written.stated()),
                            claim_of(&claimed(&written.claim())),
                        );
                    }
                    let unranged = format!("{name} ({})", unranged(entry));
                    (unranged.clone(), unranged)
                })
            })
            .collect::<Vec<_>>();
        Self {
            id: stated_id(record["id"].as_str().unwrap_or_default()),
            aliases: stated_ids(&record["aliases"]),
            withdrawn: record["withdrawn"].as_str().map(typed),
            packages,
            // A record naming no package (OSV's CVE records) still names
            // the repositories its commit ranges are in.
            repositories: commit_repositories(record),
        }
    }

    /// The record's claim under its id: a second copy of one id that makes
    /// it is not stated again.
    fn claim(&self) -> String {
        format!(
            "{}|{:?}|{}",
            self.id,
            self.withdrawn,
            self.packages
                .iter()
                .map(|(_, claim)| claim.as_str())
                .collect::<Vec<_>>()
                .join(";")
        )
    }

    /// Whether `other` states what this record does, word for word: one
    /// clause can say it of both.
    fn states_as(&self, other: &Self) -> bool {
        self.withdrawn == other.withdrawn
            && self.packages == other.packages
            && self.repositories == other.repositories
    }

    /// Whether `other` is this advisory under another id: either names the
    /// other as its alias.
    fn aliased(&self, other: &Self) -> bool {
        self.aliases.contains(&other.id) || other.aliases.contains(&self.id)
    }

    /// The clause of `group`, records that state one claim alike: their ids,
    /// the aliases none of them is, and what they state.
    fn sentence(group: &[Self]) -> String {
        let ids = group
            .iter()
            .map(|clause| clause.id.clone())
            .collect::<Vec<_>>();
        let mut aliases: Vec<&String> = Vec::new();
        for alias in group.iter().flat_map(|clause| &clause.aliases) {
            if !ids.contains(alias) && !aliases.contains(&alias) {
                aliases.push(alias);
            }
        }
        let first = &group[0];
        let one = group.len() == 1;
        let mut sentence = match ids.as_slice() {
            [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => first.id.clone(),
        };
        if !aliases.is_empty() {
            let aliases = aliases
                .iter()
                .map(|alias| alias.as_str())
                .collect::<Vec<_>>();
            sentence.push_str(&format!(" (aliases {})", aliases.join(", ")));
        }
        // A withdrawn record no longer affects anything: it is stated with
        // its date, and what it stated follows in the past tense.
        if let Some(at) = &first.withdrawn {
            sentence.push_str(&format!(", withdrawn at {at},"));
        }
        let packages = first
            .packages
            .iter()
            .map(|(shown, _)| {
                if one {
                    shown.clone()
                } else {
                    shown.replace(THE_RECORD, EACH_RECORD)
                }
            })
            .collect::<Vec<_>>();
        let withdrawn = first.withdrawn.is_some();
        match (packages.is_empty(), withdrawn) {
            (true, _) if !first.repositories.is_empty() => {
                if withdrawn {
                    sentence.pop();
                }
                let verb = match (withdrawn, one) {
                    (true, _) => "named",
                    (false, true) => "names",
                    (false, false) => "name",
                };
                sentence.push_str(&format!(
                    " {verb} no package, only commit ranges in {}.",
                    first.repositories.join(", ")
                ));
            }
            (true, true) => {
                sentence.pop();
                sentence.push('.');
            }
            (true, false) => sentence.push('.'),
            (false, true) => {
                let said = if one { "it affects" } else { "they affect" };
                sentence.push_str(&format!(" stated {said} {}.", packages.join("; ")));
            }
            (false, false) => {
                let verb = if one { "affects" } else { "affect" };
                sentence.push_str(&format!(" {verb} {}.", packages.join("; ")));
            }
        }
        sentence
    }
}

/// How a clause names the record its counts and ranges are in; a clause of
/// several records that state them alike says each record holds them.
const THE_RECORD: &str = "the record";
const EACH_RECORD: &str = "each record";

/// The repositories of a record's `GIT` ranges in entries that name no
/// package, each once, in the record's order.
fn commit_repositories(record: &Value) -> Vec<String> {
    let mut repositories = Vec::new();
    let entries = record["affected"].as_array().into_iter().flatten();
    for entry in entries.filter(|entry| entry.get("packageName").is_none()) {
        let ranges = entry["ranges"].as_array().into_iter().flatten();
        for repo in ranges
            .filter(|range| range["type"] == "GIT")
            .filter_map(|range| range["repo"].as_str())
        {
            let repo = typed(repo);
            if !repositories.contains(&repo) {
                repositories.push(repo);
            }
        }
    }
    repositories
}

/// A range's claim as both sources make it alike: with a fix named, the
/// versions from its lower bound up to that fix, however its upper bound is
/// written (OSV's `<2.14.2`, GitHub's `<= 2.14.1`, both `fixed 2.14.2`);
/// without one, the range as written.
fn claimed(range: &str) -> String {
    match range.rsplit_once(" (") {
        Some((bounds, fix)) if fix.starts_with("fixed ") => {
            let lower = bounds
                .split(' ')
                .filter(|part| !part.starts_with('<'))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{lower} ({fix}")
        }
        _ => range.to_owned(),
    }
}

/// What an entry with no version range holds, as the answer names it.
fn unranged(entry: &Value) -> String {
    let git = entry["ranges"]
        .as_array()
        .is_some_and(|ranges| ranges.iter().any(|range| range["type"] == "GIT"));
    let versions = entry["versions"].as_array().map_or(0, Vec::len);
    let written = entry["ecosystemSpecific"]["vulnerable_version_range"].is_string();
    match (listed_versions(versions), git, written) {
        (Some(listed), _, _) => listed,
        (None, true, _) => format!("git commit ranges in {THE_RECORD}"),
        // A range GitHub wrote that is not comparators alone: stated in
        // `affected`, never in the sentence.
        (None, false, true) => "a range GitHub writes as text, in affected".to_owned(),
        (None, false, false) => format!("no ranges or versions in {THE_RECORD}"),
    }
}

/// A vulnerable range GitHub writes as text (`>= 4.0.0, < 4.17.21`, kept under
/// `ecosystemSpecific`), as the sentence states it: each comparator with its
/// version typed, then the first patched version. `None` when the entry has no
/// such range or a part of it is not a comparator, so no source prose reaches
/// the answer.
fn written_range(entry: &Value) -> Option<WrittenRange> {
    let specific = &entry["ecosystemSpecific"];
    let range = specific["vulnerable_version_range"].as_str()?;
    let parts = range
        .split(',')
        .map(|part| {
            let part = part.trim();
            let operator = part.len() - part.trim_start_matches(['<', '>', '=']).len();
            let (operator, version) = part.split_at(operator);
            let version = version.trim();
            (matches!(operator, "" | "=" | "<" | "<=" | ">" | ">=")
                && !version.is_empty()
                && typed(version) == version)
                .then(|| format!("{operator}{version}"))
        })
        .collect::<Option<Vec<_>>>()?;
    // A repository advisory names its fix in `patched_versions`, as text.
    // A list (`2.0.0, 1.5.3`) states each version; a part that is not a
    // bare version states none of it.
    let patched = specific["patched_versions"].as_str().and_then(|patched| {
        patched
            .split(',')
            .map(str::trim)
            .map(|part| (!part.is_empty() && typed(part) == part).then(|| part.to_owned()))
            .collect::<Option<Vec<_>>>()
    });
    let fix = match (specific["first_patched_version"].as_str(), patched) {
        (Some(fixed), _) => format!("fixed {}", typed(fixed)),
        (None, Some(listed)) => format!("fixed {}", listed.join(", ")),
        (None, None) => "no fix named".to_owned(),
    };
    Some(WrittenRange {
        range: parts.join(" "),
        fix,
    })
}

/// [`written_range`]'s comparators and fix.
struct WrittenRange {
    range: String,
    fix: String,
}

impl WrittenRange {
    /// As the sentence says it, naming whose words the range is.
    fn stated(&self) -> String {
        format!("{} as GitHub writes it ({})", self.range, self.fix)
    }

    /// The claim itself, as an OSV record's intervals state it.
    fn claim(&self) -> String {
        format!("{} ({})", self.range, self.fix)
    }
}

/// The one wording for an entry's explicit `versions` list, ranged or not;
/// `None` when it lists none.
fn listed_versions(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some(format!("{THE_RECORD} lists 1 affected version")),
        count => Some(format!("{THE_RECORD} lists {count} affected versions")),
    }
}

/// Each id of a list as [`stated_id`] states it.
fn stated_ids(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(stated_id)
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
pub(super) fn commit_lead(commit: &str, facts: &Facts<'_>) -> (Value, Answer) {
    let found = facts.count > 0;
    let commit = typed(commit);
    let mut answer = Answer::default();
    if facts.stopped {
        answer.push(format!("Commit {commit}: {STOPPED}"), None);
    } else if found {
        answer.push(
            format!(
                "Commit {commit} is in the affected range of {}.",
                counted(facts.count)
            ),
            Some("affected"),
        );
    } else {
        answer.push(
            format!(
                "None known to OSV for commit {commit}, which proves nothing for a commit: OSV answers the same for one it does not track."
            ),
            None,
        );
    }
    answer.push(cve_clause(&facts.cves), Some("advisories"));
    answer.push(withdrawn_clause(&facts.withdrawn), Some("advisoryCount"));
    let mut data = json!({ "answer": answer.sentence() });
    if found && !facts.stopped {
        data["affected"] = json!(true);
    }
    // CVEs are stated on each advisory; the answer names them first.
    (data, answer)
}

/// The answer's clause for withdrawn advisories: named with when, never
/// counted.
fn withdrawn_clause(withdrawn: &[(String, String)]) -> String {
    if withdrawn.is_empty() {
        return String::new();
    }
    let listed = withdrawn
        .iter()
        .map(|(id, at)| format!("{} (withdrawn at {})", stated_id(id), typed(at)))
        .collect::<Vec<_>>();
    format!(" Withdrawn and not counted: {}.", listed.join(", "))
}

/// Ready calls that read each record disputing a listed one whole: the
/// dispute warning says to read its ranges, and the row does not hold it. A
/// listed record needs none: the `details` view states each one whole, and
/// the summary names the view that does.
pub(super) fn read_disputing(
    data: &mut Value,
    results: &[SourceResult],
    brief: &Map<String, Value>,
) {
    let listed = results
        .iter()
        .flat_map(|result| &result.advisories)
        .map(|row| row.id.as_str())
        .collect::<Vec<_>>();
    let mut ids: Vec<&str> = Vec::new();
    for id in results
        .iter()
        .flat_map(|result| &result.advisories)
        .flat_map(|row| row.disputed_by.iter().map(String::as_str))
    {
        if !listed.contains(&id) && !ids.contains(&id) {
            ids.push(id);
        }
    }
    advisory_calls(data, "readDisputing", &ids, brief, false);
}

/// Ready `advisory` calls under `{key}{index}`, one row per id the published
/// contract takes as an `id`. An id a source states is data: one the contract
/// refuses (a space, prose) stays named on its row and gets no call, as no
/// call could carry it. `needed`: the row names these records and states
/// nothing else of them, so their reads stay on an answer
/// ([`Continuation::required`]).
pub(super) fn advisory_calls(
    data: &mut Value,
    key: &str,
    ids: &[&str],
    brief: &Map<String, Value>,
    needed: bool,
) {
    let rows = ids
        .iter()
        .map(|id| {
            // The read of a record whole: the advisory operation's `details`.
            let mut row = super::summary::with_details(brief.clone());
            row.insert("operation".into(), json!("advisory"));
            row.insert("id".into(), json!(id));
            Value::Object(row)
        })
        .filter(|row| {
            octocode_runtime_base::contracts::validate(
                ToolId::SecurityCheck.as_str(),
                json!({ "queries": [row] }),
            )
            .is_ok()
        })
        .collect::<Vec<_>>();
    // One call per row limit of ids, so every listed advisory has a ready
    // read: a lean row leaves its text and ranges to that read.
    let per_call = octocode_runtime_base::response::result::rows_per_call(ToolId::SecurityCheck);
    for (index, chunk) in rows.chunks(per_call).enumerate() {
        let call = Continuation::related_input(ToolId::SecurityCheck, json!({ "queries": chunk }));
        data["calls"][format!("{key}{index}")] = if needed {
            call.required().build()
        } else {
            call.build()
        };
    }
}

/// The answer names each caution of level `review` the row now holds by its
/// rule, once, so a sentence that says "none known" or "OSV did not answer"
/// does not stand alone beside cautions to look at; with none, the answer
/// stays as it is. Rule names come from the contract; the raw text stays in
/// `cautions[].why`. Shared by every writer of cautions (trust, malware) and
/// by the OSV-failed rewrite.
pub(super) fn name_review_cautions(data: &mut Value) {
    const LEAD: &str = " Review before relying on it: ";
    let mut rules: Vec<String> = Vec::new();
    for caution in data["cautions"].as_array().into_iter().flatten() {
        if caution["level"] == "review"
            && let Some(rule) = caution["rule"].as_str()
            && !rules.iter().any(|known| known == rule)
        {
            rules.push(rule.to_owned());
        }
    }
    let Some(answer) = data["answer"].as_str() else {
        return;
    };
    if rules.is_empty() {
        return;
    }
    let kept = match answer.split_once(LEAD) {
        Some((before, rest)) => {
            let after = rest
                .split_once("; see cautions.")
                .map_or("", |(_, after)| after);
            format!("{before}{after}")
        }
        None => answer.to_owned(),
    };
    data["answer"] = json!(format!("{kept}{LEAD}{}; see cautions.", rules.join(", ")));
}
