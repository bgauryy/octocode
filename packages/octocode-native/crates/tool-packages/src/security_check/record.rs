//! One OSV record as an advisory row.
use super::ecosystem::{Ecosystem, osv_name};
use crate::artifact::ArtifactError;
use serde::Serialize;
use serde_json::{Map, Value, json};

/// Each source is serialized independently; unstated fields and selected empty
/// collections are omitted.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SecurityAdvisory {
    pub id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    pub link: String,
    /// The database this copy of the advisory comes from. Each database keeps
    /// its own record, with its own severity, text and dates: the copies of
    /// one id are told apart by this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
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

/// OSV's record of an advisory.
pub(super) const OSV_DATABASE: &str = "OSV";
/// GitHub's global advisory database (`github.com/advisories`).
pub(super) const GITHUB_DATABASE: &str = "GitHub advisory database";
/// A repository's own advisory on GitHub (`<repo>/security/advisories`).
pub(super) const GITHUB_REPOSITORY_DATABASE: &str = "GitHub repository advisory";

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
            // No word from the database: its vectors as published. A score
            // computed from them only ranks the row ([`super::cvss`]); none
            // is stated.
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
/// states (`critical`, `high`, `moderate` or `medium`, `low`), or, with no
/// word, the CVSS rating of its vectors ([`ranked_severity`]); a row with
/// another word or neither comes last. Rows of one severity keep their order.
pub(super) fn most_severe_first(rows: &mut [Value]) {
    rows.sort_by_key(severity_rank);
}

/// The severity a row ranks by: its database's word, else the qualitative
/// rating of its CVSS vectors' base score ([`super::cvss::rating`]), which
/// ranks the row and states no score.
pub(super) fn ranked_severity(row: &Value) -> Option<&str> {
    match row["severity"].as_str() {
        Some(word) => Some(word),
        None => super::cvss::rating(row["cvss"].as_array()?),
    }
}

/// Where a row's severity stands ([`ranked_severity`]), the most severe lowest.
pub(super) fn severity_rank(row: &Value) -> u8 {
    match ranked_severity(row) {
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

/// The records of `rows` that name one advisory, grouped
/// ([`crate::artifact::alias_groups`]): a GHSA record and its
/// PYSEC or GO twin are one advisory, not two.
pub(super) fn advisory_groups<'a>(rows: &[&'a SecurityAdvisory]) -> Vec<Vec<&'a SecurityAdvisory>> {
    crate::artifact::alias_groups(rows, |row| {
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
        Ecosystem::Pypi => {
            let mut folded = String::with_capacity(name.len());
            for part in name.split(['-', '_', '.']).filter(|part| !part.is_empty()) {
                if !folded.is_empty() {
                    folded.push('-');
                }
                folded.push_str(&part.to_ascii_lowercase());
            }
            folded
        }
        Ecosystem::Nuget => name.to_ascii_lowercase(),
        Ecosystem::Crates => name.to_ascii_lowercase().replace('_', "-"),
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
    // A record states OSV's name; the selection holds the input's.
    let asked = super::ecosystem::osv(*ecosystem);
    let same_ecosystem =
        stated_ecosystem.is_some_and(|stated| stated.split(':').next() == Some(asked.as_str()));
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
        (true, Some(release))
            if !stated_ecosystem.is_some_and(|stated| {
                states_release(stated, &osv_name(*ecosystem, Some(release)))
            }) =>
        {
            Scope::OtherRelease
        }
        (true, _) => Scope::Own,
    }
}

/// Whether the ecosystem an entry states is the asked release: equal, or one
/// whose colon components begin with the asked ones (`Ubuntu:22.04:LTS` is
/// `Ubuntu:22.04`; `Ubuntu:Pro:22.04:LTS` is not).
fn states_release(stated: &str, asked: &str) -> bool {
    let mut stated = stated.split(':');
    asked.split(':').all(|part| stated.next() == Some(part))
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
        database: Some(OSV_DATABASE.to_owned()),
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
        fixed_in: if names_one_package(&own) {
            fixed_in(&own)
        } else {
            Vec::new()
        },
        withdrawn: text(fields.get("withdrawn")),
        also_affects: also_affects(&other),
        disputed_by: Vec::new(),
        affected: own.iter().map(|entry| affected_row(entry)).collect(),
        references: list(fields.get("references")).to_vec(),
        id,
    })
}

/// One OSV severity entry as the row states it: OSV names the vector `score`,
/// the row names it `vector`. OSV gives no number, so none is stated; a source
/// that publishes one sets it on the row, and one computed from the vector
/// only ranks a record with no severity word ([`super::cvss`]). Only a
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

/// Whether `own` entries name one package: only then is one list of fixed
/// versions an unambiguous fix. A record that affects several packages states
/// each package's fix in its `affected` entries and in the answer.
fn names_one_package(own: &[&Value]) -> bool {
    let mut names = own
        .iter()
        .filter_map(|entry| entry.pointer("/package/name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    names.len() <= 1
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

/// The source's package identity fields and their flat output names.
/// Missing identity stays missing; no ecosystem or purl is inferred. The
/// ecosystem is shown as the `ecosystem` input spells it
/// ([`super::ecosystem::as_input`]), so a row's value runs as the next call's.
const PACKAGE_IDENTITY: [(&str, &str); 3] = [
    ("name", "packageName"),
    ("ecosystem", "ecosystem"),
    ("purl", "purl"),
];

/// An entry with its source package identity intact. A named selection may
/// still hold distribution releases or purl qualifiers its request did not
/// state. Each entry keeps its opaque metadata and original attribution.
fn affected_row(entry: &Value) -> Value {
    let mut row = Map::new();
    for (from, to) in PACKAGE_IDENTITY {
        if let Some(value) = entry
            .get("package")
            .and_then(|package| package.get(from))
            .filter(|value| value.is_string())
        {
            let value = match value.as_str() {
                Some(stated) if from == "ecosystem" => {
                    json!(super::ecosystem::as_input(stated))
                }
                _ => value.clone(),
            };
            row.insert(to.to_owned(), value);
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
        let value = value.clone();
        let holds_nothing = value.is_null()
            || value.as_array().is_some_and(Vec::is_empty)
            || value.as_object().is_some_and(Map::is_empty);
        if !holds_nothing {
            row.insert(to.to_owned(), value);
        }
    }
    Value::Object(row)
}

/// Drops the `link` of each details row that is the OSV page its `id` names
/// (`https://osv.dev/vulnerability/{id}`): the output description states that
/// form once, and any other link or `source` stays.
pub(super) fn drop_derivable_links(rows: &mut [Value]) {
    for row in rows {
        let derivable = match (row.get("id").and_then(Value::as_str), row.get("link")) {
            (Some(id), Some(link)) => link.as_str() == Some(record_url(id).as_str()),
            _ => false,
        };
        if let (true, Some(fields)) = (derivable, row.as_object_mut()) {
            fields.shift_remove("link");
        }
    }
}

/// Drops a details record's `fixedIn` only when its own affected fixed
/// events already state every selected fix. Other records cannot supply them.
pub(super) fn drop_restated_fixes(rows: &mut [Value]) {
    for row in rows {
        let events = list(row.get("affected"))
            .iter()
            .flat_map(|entry| list(entry.get("ranges")))
            .flat_map(|range| list(range.get("events")))
            .filter_map(|event| event.get("fixed"))
            .collect::<Vec<_>>();
        let restated = row
            .get("fixedIn")
            .and_then(Value::as_array)
            .is_some_and(|fixes| fixes.iter().all(|fix| events.contains(&fix)));
        if let (true, Some(fields)) = (restated, row.as_object_mut()) {
            fields.shift_remove("fixedIn");
        }
    }
}

/// The text fields one advisory's records often carry alike: OSV's record,
/// GitHub's database and the repository's advisory (the last with `\r\n`
/// line endings).
const TEXT_FIELDS: [&str; 2] = ["summary", "details"];

/// States each advisory's text once across its details records: a later
/// record of an id drops each of `summary` and `details` equal to an earlier
/// record's, line endings aside, and names that record by its `link` in
/// `textSameAs`. Only a record that still holds all its own text is pointed
/// to, so the pointer always resolves to the text; text that differs stays.
pub(super) fn drop_restated_text(rows: &mut [Value]) {
    let unified = |row: &Value, field: &str| {
        row.get(field)
            .and_then(Value::as_str)
            .map(|text| text.replace("\r\n", "\n"))
    };
    // Records that hold their own text: (id, link, each text field).
    let mut holders: Vec<(String, String, [Option<String>; 2])> = Vec::new();
    for row in rows {
        let (Some(id), Some(link)) = (
            row.get("id").and_then(Value::as_str).map(str::to_owned),
            row.get("link").and_then(Value::as_str).map(str::to_owned),
        ) else {
            continue;
        };
        let texts = TEXT_FIELDS.map(|field| unified(row, field));
        let same =
            |held: &[Option<String>; 2], at: usize| texts[at].is_some() && held[at] == texts[at];
        let holder = holders.iter().find(|(held_id, _, held)| {
            *held_id == id && (0..TEXT_FIELDS.len()).any(|at| same(held, at))
        });
        let Some((_, holder_link, held)) = holder else {
            holders.push((id, link, texts));
            continue;
        };
        let restated = (0..TEXT_FIELDS.len())
            .filter(|at| same(held, *at))
            .collect::<Vec<_>>();
        let pointer = holder_link.clone();
        if let Some(fields) = row.as_object_mut() {
            let at = fields
                .keys()
                .position(|key| key == TEXT_FIELDS[restated[0]])
                .unwrap_or(fields.len());
            for field in restated {
                fields.shift_remove(TEXT_FIELDS[field]);
            }
            fields.shift_insert(
                at.min(fields.len()),
                "textSameAs".to_owned(),
                json!(pointer),
            );
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
