//! One OSV record as an advisory row.
use super::osv::ecosystem;
use crate::providers::artifact::ArtifactError;
use octocode_contract_types::ArtifactType;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cvss: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cwes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixed_in: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdrawn: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also_affects: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub affected: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Value>,
}

/// Which `affected` entries a row states as its own.
pub(super) enum Selection<'a> {
    /// An advisory or commit named directly: every entry.
    All,
    /// Entries about these packages of one ecosystem; others are named in
    /// `alsoAffects`.
    Named(ArtifactType, &'a [String]),
}

impl SecurityAdvisory {
    /// The CVE ids this record is or names.
    pub(super) fn cves(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .filter(|id| is_cve(id))
    }

    /// What decides an upgrade: the record's id, its CVEs, one line of what it
    /// is, the database's severity word and that database, the fixed versions,
    /// the link. All the rest is one `advisory` call away.
    pub(super) fn brief(&self) -> Value {
        let mut row = json!({"id": self.id});
        let cves = self.cves().collect::<Vec<_>>();
        if !cves.is_empty() {
            row["cves"] = json!(cves);
        }
        if let Some(line) = self.one_line() {
            row["summary"] = json!(line);
        }
        match &self.severity {
            Some(severity) => row["severity"] = json!(severity),
            // No word from the database: its vectors, never a computed score.
            None if !self.cvss.is_empty() => row["cvss"] = json!(self.cvss),
            None => {}
        }
        if let Some(source) = &self.source {
            row["source"] = json!(source);
        }
        if !self.fixed_in.is_empty() {
            row["fixedIn"] = json!(self.fixed_in);
        }
        if let Some(withdrawn) = &self.withdrawn {
            row["withdrawn"] = json!(withdrawn);
        }
        row["link"] = json!(self.link);
        row
    }

    /// Its summary, else the first line of its details; a database that gives
    /// neither gives no line.
    fn one_line(&self) -> Option<&str> {
        self.summary
            .as_deref()
            .or_else(|| {
                self.details
                    .as_deref()?
                    .lines()
                    .find(|line| !line.trim().is_empty())
            })
            .map(str::trim)
    }
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

/// One brief row for one advisory's records: the first's brief, with every
/// other record's id in `aliases` (each one `advisory` call away) and every
/// CVE and fixed version of the group.
pub(super) fn group_brief(group: &[&SecurityAdvisory]) -> Value {
    let Some((first, twins)) = group.split_first() else {
        return Value::Null;
    };
    let mut row = first.brief();
    if twins.is_empty() {
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
        twins.iter().map(|twin| twin.id.as_str()).collect(),
    );
    add(
        "cves",
        group.iter().flat_map(|record| record.cves()).collect(),
    );
    add(
        "fixedIn",
        group
            .iter()
            .flat_map(|record| record.fixed_in.iter().map(String::as_str))
            .collect(),
    );
    if row.get("severity").is_none()
        && let Some(severity) = twins.iter().find_map(|twin| twin.severity.as_deref())
    {
        row["severity"] = json!(severity);
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
pub(super) fn comparable(kind: ArtifactType, name: &str) -> String {
    match kind {
        ArtifactType::Pypi => {
            let mut folded = String::with_capacity(name.len());
            for part in name.split(['-', '_', '.']).filter(|part| !part.is_empty()) {
                if !folded.is_empty() {
                    folded.push('-');
                }
                folded.push_str(&part.to_ascii_lowercase());
            }
            folded
        }
        ArtifactType::Nuget => name.to_ascii_lowercase(),
        ArtifactType::Crates => name.to_ascii_lowercase().replace('_', "-"),
        ArtifactType::Npm
        | ArtifactType::Maven
        | ArtifactType::Go
        | ArtifactType::Packagist
        | ArtifactType::Rubygems => name.to_owned(),
    }
}

fn is_own(entry: &Value, selection: &Selection<'_>) -> bool {
    let Selection::Named(kind, names) = selection else {
        return true;
    };
    let package = entry.get("package");
    let same_ecosystem = package
        .and_then(|package| package.get("ecosystem"))
        .and_then(Value::as_str)
        .is_some_and(|stated| stated.split(':').next() == Some(ecosystem(*kind)));
    same_ecosystem
        && package
            .and_then(|package| package.get("name"))
            .and_then(Value::as_str)
            .is_some_and(|stated| {
                let stated = comparable(*kind, stated);
                names.iter().any(|name| comparable(*kind, name) == stated)
            })
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
    let (own, other): (Vec<&Value>, Vec<&Value>) = list(fields.get("affected"))
        .iter()
        .partition(|entry| is_own(entry, selection));
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
            .map(|label| label.to_ascii_lowercase()),
        aliases: texts(fields.get("aliases")),
        related: texts(fields.get("related")),
        cvss: list(fields.get("severity")).to_vec(),
        cwes: texts(database.and_then(|value| value.get("cwe_ids"))),
        summary: text(fields.get("summary")),
        details: text(fields.get("details")),
        published: text(fields.get("published")),
        modified: text(fields.get("modified")),
        fixed_in: fixed_in(&own),
        withdrawn: text(fields.get("withdrawn")),
        also_affects: also_affects(&other),
        affected: own.iter().map(|entry| affected_row(entry)).collect(),
        references: list(fields.get("references")).to_vec(),
        id,
    })
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

/// An entry as the record states it, without its repeated identity;
/// `database_specific.source` is the row's own `source`.
fn affected_row(entry: &Value) -> Value {
    let mut row = Map::new();
    if let Some(name) = text(entry.pointer("/package/name")) {
        row.insert("packageName".to_owned(), Value::String(name));
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
    Value::Object(row)
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
