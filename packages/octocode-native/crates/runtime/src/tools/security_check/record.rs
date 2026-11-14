//! One OSV record as an advisory row.
use super::ecosystem::{Ecosystem, osv_name};
use crate::providers::artifact::ArtifactError;
use serde::Serialize;
use serde_json::{Map, Value, json};

/// Absent fields are ones the source did not state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SecurityAdvisory {
    pub id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    pub link: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// GitHub's class of the record (`reviewed`, `unreviewed`, `malware`);
    /// absent: not GitHub's database, or it states none of these.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_type: Option<String>,
    /// The GitHub account that published a repository advisory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Who the source credits: `{login, type}`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cvss: Vec<Value>,
    /// GitHub's EPSS (`percentage`, `percentile`); OSV states none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epss: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cwes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixed_in: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdrawn: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also_affects: Vec<String>,
    /// Aliases kept for the same package by another database that OSV does
    /// not list at the version asked: the databases disagree.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disputed_by: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub affected: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Value>,
}

/// Which `affected` entries a row states as its own.
pub(super) enum Selection<'a> {
    /// An advisory or commit named directly: every entry.
    All,
    /// Entries about these packages of one ecosystem (of one release, when a
    /// distribution release was asked); other packages are named in
    /// `alsoAffects`, and the same package in another release is left out.
    Named(Ecosystem, &'a [String], Option<&'a str>),
}

impl SecurityAdvisory {
    /// The CVE ids this record is or names.
    pub(super) fn cves(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .filter(|id| is_cve(id))
    }

    /// GitHub's class of the record when it changes how far to rely on it
    /// (`unreviewed`, `malware`); `reviewed` is the class that needs no word.
    fn cautioned_class(&self) -> Option<&str> {
        self.github_type
            .as_deref()
            .filter(|class| *class != "reviewed")
    }

    /// What decides: the record's id, its CVEs, the database's severity word,
    /// the fixed versions, a withdrawal or dispute, and the link to read it
    /// at; GitHub's class when it is not `reviewed`. All the rest (summary,
    /// details, ranges, references, EPSS, the database's own file) is one `advisory` call away, which the
    /// row offers once for the whole list.
    pub(super) fn brief(&self) -> Value {
        let mut row = json!({"id": self.id});
        let cves = self.cves().collect::<Vec<_>>();
        if !cves.is_empty() {
            row["cves"] = json!(cves);
        }
        match &self.severity {
            Some(severity) => row["severity"] = json!(severity),
            // No word from the database: its vectors, never a computed score.
            None if !self.cvss.is_empty() => row["cvss"] = json!(self.cvss),
            None => {}
        }
        if !self.fixed_in.is_empty() {
            row["fixedIn"] = json!(self.fixed_in);
        }
        if let Some(class) = self.cautioned_class() {
            row["githubType"] = json!(class);
        }
        if let Some(withdrawn) = &self.withdrawn {
            row["withdrawn"] = json!(withdrawn);
        }
        if !self.disputed_by.is_empty() {
            row["disputedBy"] = json!(self.disputed_by);
        }
        // One reference: the link, or the database's file when there is none.
        if !self.link.is_empty() {
            row["link"] = json!(self.link);
        } else if let Some(source) = &self.source {
            row["source"] = json!(source);
        }
        row
    }
}

/// Brief rows with the most severe first, by the severity word each database
/// states (`critical`, `high`, `moderate` or `medium`, `low`); a row with
/// another word or none comes last. Rows of one severity keep their order.
pub(super) fn most_severe_first(rows: &mut [Value]) {
    rows.sort_by_key(severity_rank);
}

/// Where a row's severity word stands, the most severe lowest.
pub(super) fn severity_rank(row: &Value) -> u8 {
    match row["severity"].as_str() {
        Some("critical") => 0,
        Some("high") => 1,
        Some("moderate" | "medium") => 2,
        Some("low") => 3,
        _ => 4,
    }
}

/// One row for one advisory in the `summary` view of the advisory read:
/// its brief with the database's one-line summary. The ranges and fixes are
/// in the row's `answer`; the text, references and scores are `details`.
pub(super) fn group_summary(group: &[&SecurityAdvisory]) -> Value {
    let mut row = group_brief(group);
    let stated = group.iter().find_map(|record| record.summary.as_deref());
    if let (Some(fields), Some(summary)) = (row.as_object_mut(), stated) {
        // Before the one reference, which a brief row states last.
        let at = fields
            .keys()
            .position(|key| key == "link" || key == "source")
            .unwrap_or(fields.len());
        fields.shift_insert(at, "summary".to_owned(), json!(summary));
    }
    row
}

/// The `details` rows of one advisory that OSV and GitHub both keep under
/// one id: the first states every field, and each later row of that id only
/// the fields its source states otherwise (its own link, ranges, scores),
/// so text both carry alike (summary, details, aliases) is read once. A
/// difference of notation is none: an `affected` entry of the same range
/// ([`same_range`]) and a reference to a URL already stated are left out. The
/// ids so stated, each once.
pub(super) fn state_twin_differences(rows: &mut [Value]) -> Vec<String> {
    let mut twins: Vec<String> = Vec::new();
    for at in 1..rows.len() {
        let (before, rest) = rows.split_at_mut(at);
        let Some(row) = rest.first_mut() else {
            continue;
        };
        let Some(id) = row["id"].as_str().map(str::to_owned) else {
            continue;
        };
        let Some(first) = before.iter().find(|earlier| earlier["id"] == id.as_str()) else {
            continue;
        };
        if let Some(fields) = row.as_object_mut() {
            fields.retain(|key, value| key == "id" || first.get(key) != Some(&*value));
            let ranges = list(first.get("affected"));
            keep_new(fields, "affected", |entry| {
                !ranges
                    .iter()
                    .any(|stated| same_range(entry, stated) || same_range(stated, entry))
            });
            let mut urls = list(first.get("references"))
                .iter()
                .filter_map(|reference| reference.get("url"))
                .chain(first.get("link"))
                .chain(fields.get("link"))
                .cloned()
                .collect::<Vec<_>>();
            urls.extend(first.get("source").cloned());
            keep_new(fields, "references", |reference| {
                reference.get("url").is_none_or(|url| !urls.contains(url))
            });
        }
        if !twins.contains(&id) {
            twins.push(id);
        }
    }
    twins
}

/// Keeps the items of the array `key` that `new` accepts; the key goes when
/// none is left.
fn keep_new(fields: &mut Map<String, Value>, key: &str, new: impl Fn(&Value) -> bool) {
    let Some(Value::Array(items)) = fields.get_mut(key) else {
        return;
    };
    items.retain(|item| new(item));
    if items.is_empty() {
        fields.shift_remove(key);
    }
}

/// Whether GitHub's `entry` (its range as text under `ecosystemSpecific`)
/// states the range OSV's `stated` does in events: the same package, the same
/// inclusive lower bound (none: OSV's `introduced 0`) and the one same fix.
/// Anything more GitHub states (another field, a second fix, an exclusive
/// bound) is a difference and stays.
fn same_range(entry: &Value, stated: &Value) -> bool {
    let Some(fields) = entry.as_object() else {
        return false;
    };
    if PACKAGE_IDENTITY
        .iter()
        .any(|(_, field)| fields.get(*field) != stated.get(*field))
        || fields.keys().any(|key| {
            key != "ecosystemSpecific"
                && !PACKAGE_IDENTITY
                    .iter()
                    .any(|(_, field)| key.as_str() == *field)
        })
    {
        return false;
    }
    let Some(specific) = fields.get("ecosystemSpecific").and_then(Value::as_object) else {
        return false;
    };
    if specific.keys().any(|key| {
        !matches!(
            key.as_str(),
            "vulnerable_version_range" | "patched_versions" | "first_patched_version"
        )
    }) {
        return false;
    }
    let Some(range) = specific
        .get("vulnerable_version_range")
        .and_then(Value::as_str)
    else {
        return false;
    };
    let lowers = super::advisories::comparators(range)
        .into_iter()
        .filter(|(operator, _)| operator.starts_with('>'))
        .collect::<Vec<_>>();
    let lower = match lowers.as_slice() {
        [] => "0",
        [(">=", version)] => version,
        _ => return false,
    };
    let mut fixes = specific
        .get("first_patched_version")
        .and_then(Value::as_str)
        .into_iter()
        .chain(
            specific
                .get("patched_versions")
                .and_then(Value::as_str)
                .into_iter()
                .flat_map(super::advisories::fixes),
        )
        .collect::<Vec<_>>();
    fixes.dedup();
    let [fix] = fixes.as_slice() else {
        return false;
    };
    let events = match list(stated.get("ranges")) {
        [range]
            if matches!(
                range.get("type").and_then(Value::as_str),
                Some("SEMVER" | "ECOSYSTEM")
            ) =>
        {
            list(range.get("events"))
        }
        _ => return false,
    };
    events == [json!({"introduced": lower}), json!({"fixed": fix})]
}

/// The records of `rows` that name one advisory, grouped
/// ([`crate::providers::artifact::alias_groups`]): a GHSA record and its
/// PYSEC or GO twin are one advisory, not two.
pub(super) fn advisory_groups<'a>(rows: &[&'a SecurityAdvisory]) -> Vec<Vec<&'a SecurityAdvisory>> {
    crate::providers::artifact::alias_groups(rows, |row| {
        std::iter::once(row.id.as_str())
            .chain(row.aliases.iter().map(String::as_str))
            .collect()
    })
    .into_iter()
    .map(|members| members.into_iter().map(|at| rows[at]).collect())
    .collect()
}

/// How one advisory's records stand on withdrawal. The one rule, for package,
/// commit and repository rows alike: the records of one id are one record
/// (OSV's and GitHub's twin of it), withdrawn when either states a withdrawal;
/// the advisory is withdrawn only when every record in it is. A live record
/// always keeps the advisory live, and a withdrawn duplicate that shares its
/// CVE is stated apart, never counted and never hiding it.
pub(super) struct Standing<'a> {
    /// The records of ids no record withdraws, in the group's order.
    pub live: Vec<&'a SecurityAdvisory>,
    /// Each withdrawn id once, with when.
    pub withdrawn: Vec<(&'a str, &'a str)>,
}

impl<'a> Standing<'a> {
    pub(super) fn of(group: &[&'a SecurityAdvisory]) -> Self {
        let mut standing = Self {
            live: Vec::new(),
            withdrawn: Vec::new(),
        };
        for record in group {
            let when = group
                .iter()
                .filter(|twin| twin.id == record.id)
                .find_map(|twin| twin.withdrawn.as_deref());
            match when {
                None => standing.live.push(record),
                Some(at) if !standing.withdrawn.iter().any(|(id, _)| *id == record.id) => {
                    standing.withdrawn.push((record.id.as_str(), at));
                }
                Some(_) => {}
            }
        }
        standing
    }

    /// No record of the advisory is live: it is not counted.
    pub(super) fn is_withdrawn(&self) -> bool {
        self.live.is_empty()
    }
}

/// The id a group's brief row leads with: its first live record's.
pub(super) fn lead_id<'a>(group: &[&'a SecurityAdvisory]) -> Option<&'a str> {
    let standing = Standing::of(group);
    let stated = if standing.is_withdrawn() {
        group.to_vec()
    } else {
        standing.live
    };
    stated.first().map(|record| record.id.as_str())
}

/// One brief row for one advisory's records: the first's brief, with every
/// other record's id in `aliases` (each one `advisory` call away) and every
/// CVE and fixed version of the group.
pub(super) fn group_brief(group: &[&SecurityAdvisory]) -> Value {
    // The row states the advisory's live records: a withdrawn duplicate leads
    // nothing and claims no CVE or fix. With none live, every record is
    // withdrawn and all of them speak.
    let standing = Standing::of(group);
    let stated = if standing.is_withdrawn() {
        group.to_vec()
    } else {
        standing.live
    };
    let Some((first, twins)) = stated.split_first() else {
        return Value::Null;
    };
    let mut row = first.brief();
    if group.len() == 1 {
        return row;
    }
    let mut add = |key: &str, values: Vec<&str>| {
        let mut listed = row[key].as_array().cloned().unwrap_or_default();
        for value in values {
            if !listed.iter().any(|known| known == value) {
                listed.push(json!(value));
            }
        }
        if !listed.is_empty() {
            row[key] = Value::Array(listed);
        }
    };
    add(
        "aliases",
        group
            .iter()
            .map(|record| record.id.as_str())
            .filter(|id| *id != first.id)
            .collect(),
    );
    add(
        "cves",
        stated.iter().flat_map(|record| record.cves()).collect(),
    );
    add(
        "disputedBy",
        stated
            .iter()
            .flat_map(|record| record.disputed_by.iter().map(String::as_str))
            .collect(),
    );
    add(
        "fixedIn",
        stated
            .iter()
            .flat_map(|record| record.fixed_in.iter().map(String::as_str))
            .collect(),
    );
    // A withdrawal either record states is the advisory's.
    if row.get("withdrawn").is_none()
        && let Some(withdrawn) = twins.iter().find_map(|twin| twin.withdrawn.as_deref())
    {
        row["withdrawn"] = json!(withdrawn);
    }
    if row.get("severity").is_none()
        && let Some(severity) = twins.iter().find_map(|twin| twin.severity.as_deref())
    {
        row["severity"] = json!(severity);
    }
    // GitHub's class of the advisory comes with its twin of an OSV record.
    if row.get("githubType").is_none()
        && let Some(class) = twins.iter().find_map(|twin| twin.cautioned_class())
    {
        row["githubType"] = json!(class);
    }
    // `link` stays last, as a brief row states it.
    if let Some(link) = row.as_object_mut().and_then(|fields| fields.remove("link")) {
        row["link"] = link;
    }
    row
}

/// The repositories a record's `GIT` ranges name.
pub(super) fn repositories(row: &SecurityAdvisory) -> impl Iterator<Item = &str> {
    row.affected
        .iter()
        .flat_map(|entry| list(entry.get("ranges")))
        .filter(|range| range.get("type").and_then(Value::as_str) == Some("GIT"))
        .filter_map(|range| range.get("repo").and_then(Value::as_str))
}

/// `CVE-2021-44228`, and nothing a record could write beside it.
fn is_cve(id: &str) -> bool {
    id.strip_prefix("CVE-")
        .and_then(|rest| rest.split_once('-'))
        .is_some_and(|(year, number)| {
            year.len() == 4
                && number.len() >= 4
                && year
                    .bytes()
                    .chain(number.bytes())
                    .all(|byte| byte.is_ascii_digit())
        })
}

fn record_url(id: &str) -> String {
    format!("https://osv.dev/vulnerability/{id}")
}

/// A package name as OSV compares it: PEP 503 on PyPI, case-insensitive on
/// NuGet, case-insensitive with `-` equal to `_` on crates.io, exact elsewhere.
pub(super) fn comparable(ecosystem: Ecosystem, name: &str) -> String {
    match ecosystem {
        Ecosystem::PyPi => {
            let mut folded = String::with_capacity(name.len());
            for part in name.split(['-', '_', '.']).filter(|part| !part.is_empty()) {
                if !folded.is_empty() {
                    folded.push('-');
                }
                folded.push_str(&part.to_ascii_lowercase());
            }
            folded
        }
        Ecosystem::NuGet => name.to_ascii_lowercase(),
        Ecosystem::CratesIo => name.to_ascii_lowercase().replace('_', "-"),
        _ => name.to_owned(),
    }
}

/// Where an `affected` entry belongs for the row's selection.
enum Scope {
    /// The row's own package (and release).
    Own,
    /// Another package: named in `alsoAffects`.
    Other,
    /// The row's package in a release that was not asked.
    OtherRelease,
}

fn scope(entry: &Value, selection: &Selection<'_>) -> Scope {
    let Selection::Named(ecosystem, names, release) = selection else {
        return Scope::Own;
    };
    let package = entry.get("package");
    let stated_ecosystem = package
        .and_then(|package| package.get("ecosystem"))
        .and_then(Value::as_str);
    let same_ecosystem = stated_ecosystem
        .is_some_and(|stated| stated.split(':').next() == Some(&ecosystem.to_string()));
    let same_package = package
        .and_then(|package| package.get("name"))
        .and_then(Value::as_str)
        .is_some_and(|stated| {
            let stated = comparable(*ecosystem, stated);
            names
                .iter()
                .any(|name| comparable(*ecosystem, name) == stated)
        });
    match (same_ecosystem && same_package, release) {
        (false, _) => Scope::Other,
        (true, Some(release)) if stated_ecosystem != Some(&osv_name(*ecosystem, Some(release))) => {
            Scope::OtherRelease
        }
        (true, _) => Scope::Own,
    }
}

/// The `affected` entries of `record` that are the selection's own package.
pub(super) fn own_entries<'a>(record: &'a Value, selection: &Selection<'_>) -> Vec<&'a Value> {
    list(record.get("affected"))
        .iter()
        .filter(|entry| matches!(scope(entry, selection), Scope::Own))
        .collect()
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn list(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn texts(value: Option<&Value>) -> Vec<String> {
    list(value)
        .iter()
        .filter_map(|item| text(Some(item)))
        .collect()
}

/// One record as a row. A record without an `id`, or of a schema major this
/// does not read, is a malformed answer: the caller fails whole, so a record
/// is never silently dropped.
pub(super) fn advisory(
    record: &Value,
    selection: &Selection<'_>,
) -> Result<SecurityAdvisory, String> {
    let fields = record
        .as_object()
        .ok_or_else(|| "a record that is not an object".to_owned())?;
    let id = text(fields.get("id")).ok_or_else(|| "a record with no id".to_owned())?;
    if let Some(version) = text(fields.get("schema_version"))
        && version.split('.').next() != Some("1")
    {
        return Err(format!("record {id} in schema version {version}"));
    }
    let (mut own, mut other) = (Vec::<&Value>::new(), Vec::<&Value>::new());
    for entry in list(fields.get("affected")) {
        match scope(entry, selection) {
            Scope::Own => own.push(entry),
            Scope::Other => other.push(entry),
            Scope::OtherRelease => {}
        }
    }
    let database = fields.get("database_specific");
    Ok(SecurityAdvisory {
        link: record_url(&id),
        source: own
            .iter()
            .chain(other.iter())
            .find_map(|entry| text(entry.pointer("/database_specific/source")))
            .or_else(|| text(database.and_then(|value| value.get("source"))))
            .or_else(|| text(database.and_then(|value| value.get("url")))),
        severity: text(database.and_then(|value| value.get("severity")))
            .or_else(|| {
                own.iter()
                    .find_map(|entry| text(entry.pointer("/ecosystem_specific/severity")))
            })
            .or_else(|| list(fields.get("severity")).iter().find_map(priority_word))
            .map(|label| label.to_ascii_lowercase()),
        aliases: texts(fields.get("aliases")),
        related: texts(fields.get("related")),
        cvss: list(fields.get("severity"))
            .iter()
            .filter_map(cvss)
            .collect(),
        epss: None,
        github_type: None,
        publisher: None,
        credits: Vec::new(),
        cwes: texts(database.and_then(|value| value.get("cwe_ids"))),
        summary: text(fields.get("summary")),
        details: text(fields.get("details")),
        published_at: text(fields.get("published")),
        modified_at: text(fields.get("modified")),
        fixed_in: fixed_in(&own),
        withdrawn: text(fields.get("withdrawn")),
        also_affects: also_affects(&other),
        disputed_by: Vec::new(),
        affected: own.iter().map(|entry| affected_row(entry)).collect(),
        references: list(fields.get("references")).to_vec(),
        id,
    })
}

/// One OSV severity entry as the row states it: OSV names the vector `score`,
/// the row names it `vector`. OSV gives no number, so none is stated, and none
/// is computed here; a source that publishes one sets it on the row. Only a
/// CVSS entry is a vector: another type (Ubuntu's priority) is a word, kept by
/// [`priority_word`].
fn cvss(entry: &Value) -> Option<Value> {
    let vector = text(entry.get("score"))?;
    let kind = text(entry.get("type")).unwrap_or_else(|| cvss_type(&vector).to_owned());
    kind.starts_with("CVSS")
        .then(|| json!({ "type": kind, "vector": vector }))
}

/// The word a severity entry of a type other than CVSS states (Ubuntu's
/// `medium`): the record's severity when it states no other.
fn priority_word(entry: &Value) -> Option<String> {
    let kind = text(entry.get("type"))?;
    (!kind.starts_with("CVSS"))
        .then(|| text(entry.get("score")))
        .flatten()
}

/// OSV's name for a CVSS vector's version, read from its prefix.
pub(super) fn cvss_type(vector: &str) -> &'static str {
    match vector.split('/').next() {
        Some("CVSS:4.0") => "CVSS_V4",
        Some("CVSS:3.1" | "CVSS:3.0") => "CVSS_V3",
        _ => "CVSS_V2",
    }
}

/// `fixed` events of version ranges. A `GIT` range's `fixed` is a commit, not a
/// version, so it stays in `affected` only.
fn fixed_in(own: &[&Value]) -> Vec<String> {
    let mut versions: Vec<String> = Vec::new();
    for range in own.iter().flat_map(|entry| list(entry.get("ranges"))) {
        if !matches!(
            range.get("type").and_then(Value::as_str),
            Some("SEMVER" | "ECOSYSTEM")
        ) {
            continue;
        }
        for fixed in list(range.get("events"))
            .iter()
            .filter_map(|event| text(event.get("fixed")))
        {
            if !versions.contains(&fixed) {
                versions.push(fixed);
            }
        }
    }
    versions
}

fn also_affects(other: &[&Value]) -> Vec<String> {
    let mut names: Vec<String> = other
        .iter()
        .filter_map(|entry| text(entry.pointer("/package/name")))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// The source's package identity fields and their flat output names. Unknown
/// identity is not equality with a known value when comparing ranges.
const PACKAGE_IDENTITY: [(&str, &str); 3] = [
    ("name", "packageName"),
    ("ecosystem", "ecosystem"),
    ("purl", "purl"),
];

/// An entry with its source package identity intact. A named selection may
/// still hold distribution releases or purl qualifiers its request did not
/// state. `database_specific.source` is the row's own `source`.
fn affected_row(entry: &Value) -> Value {
    let mut row = Map::new();
    for (from, to) in PACKAGE_IDENTITY {
        if let Some(value) = entry
            .get("package")
            .and_then(|package| package.get(from))
            .filter(|value| value.is_string())
        {
            row.insert(to.to_owned(), value.clone());
        }
    }
    for (from, to) in [
        ("ranges", "ranges"),
        ("versions", "versions"),
        ("ecosystem_specific", "ecosystemSpecific"),
        ("database_specific", "databaseSpecific"),
    ] {
        let Some(value) = entry.get(from) else {
            continue;
        };
        let mut value = value.clone();
        if let Some(fields) = value.as_object_mut() {
            fields.remove("source");
        }
        let holds_nothing = value.is_null()
            || value.as_array().is_some_and(Vec::is_empty)
            || value.as_object().is_some_and(Map::is_empty);
        if !holds_nothing {
            row.insert(to.to_owned(), value);
        }
    }
    // GitHub's range text that restates the entry's events is stated once,
    // by the events.
    if let Some(Value::Object(database)) = row.get_mut("databaseSpecific") {
        if database
            .get("last_known_affected_version_range")
            .and_then(Value::as_str)
            .is_some_and(|text| super::osv::restates_range(text, entry))
        {
            database.shift_remove("last_known_affected_version_range");
        }
        if database.is_empty() {
            row.shift_remove("databaseSpecific");
        }
    }
    Value::Object(row)
}

/// Drops each `details` row's `fixedIn` when every version in it is a `fixed`
/// event of a record of its id: the ranges in `affected` and the row's
/// `answer` state those fixes already.
pub(super) fn drop_restated_fixes(rows: &mut [Value]) {
    let fixed_events = |id: &Value| {
        rows.iter()
            .filter(|row| row["id"] == *id)
            .flat_map(|row| list(row.get("affected")))
            .flat_map(|entry| list(entry.get("ranges")))
            .flat_map(|range| list(range.get("events")))
            .filter_map(|event| event.get("fixed").cloned())
            .collect::<Vec<_>>()
    };
    let restated = rows
        .iter()
        .map(|row| {
            let events = fixed_events(&row["id"]);
            row.get("fixedIn")
                .and_then(Value::as_array)
                .is_some_and(|fixes| fixes.iter().all(|fix| events.contains(fix)))
        })
        .collect::<Vec<_>>();
    for (row, restated) in rows.iter_mut().zip(restated) {
        if let (true, Some(fields)) = (restated, row.as_object_mut()) {
            fields.shift_remove("fixedIn");
        }
    }
}

/// The rows of `records`, failing whole on one malformed record.
pub(super) fn rows(
    records: &[Value],
    selection: &Selection<'_>,
    target: &str,
) -> Result<Vec<SecurityAdvisory>, ArtifactError> {
    records
        .iter()
        .map(|record| {
            advisory(record, selection).map_err(|reason| super::osv::malformed(&reason, target))
        })
        .collect()
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
