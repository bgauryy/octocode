//! Version matching for an ecosystem OSV cannot order. OSV's `/v1/query` with a
//! version matches none of such a record's ranges, so the query goes by name and
//! each record's own ranges are compared here: `SEMVER` and `ECOSYSTEM` events,
//! evaluated as OSV does (events sorted, each one applied once the version has
//! reached it) with versions read as semver. A record whose ranges or whose
//! version this cannot read stays listed and is named unmatched, so nothing is
//! dropped and nothing reads as cleared on a guess. The version asked must be
//! whole (`v4` floats to the newest 4.x, so it names no version); a record's own
//! versions may be short, as OSV writes them (`fixed: 41`). Build metadata does
//! not take part in precedence.
use super::super::record::{Selection, own_entries};
use semver::{BuildMetadata, Version};
use serde_json::Value;

/// `version` as semver precedence sees it: build metadata ignored.
fn precedence(version: Version) -> Version {
    Version {
        build: BuildMetadata::EMPTY,
        ..version
    }
}

/// The tag asked, as a semver version only when it is fully specified: a leading
/// `v` dropped, nothing padded. A tag short of its patch is no version to compare.
fn asked(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .ok()
        .map(precedence)
}

/// A version a record states, as semver: a leading `v` dropped, a missing minor
/// or patch zero.
fn parse(tag: &str) -> Option<Version> {
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    let (core, rest) = tag.split_at(tag.find(['-', '+']).unwrap_or(tag.len()));
    let mut parts = core.split('.').collect::<Vec<_>>();
    if parts.len() > 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    while parts.len() < 3 {
        parts.push("0");
    }
    Version::parse(&format!("{}{rest}", parts.join(".")))
        .ok()
        .map(precedence)
}

/// What a record says of the version.
#[derive(Debug, Eq, PartialEq)]
enum Verdict {
    Affected,
    Clear,
    /// The record's ranges or the version are not comparable here.
    Unknown,
}

/// Whether `version` is inside the intervals a range's events open and close;
/// `None` when an event's version cannot be read.
fn in_range(range: &Value, version: &Version) -> Option<bool> {
    let mut events = Vec::new();
    for event in range.get("events")?.as_array()? {
        let Some((kind, text)) = ["introduced", "fixed", "last_affected"]
            .into_iter()
            .find_map(|kind| Some((kind, event.get(kind)?.as_str()?)))
        else {
            continue;
        };
        // `0` opens a range at the lowest version there is.
        let at = if text == "0" {
            Version::parse("0.0.0-0").ok()?
        } else {
            parse(text)?
        };
        events.push((at, kind));
    }
    events.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut affected = false;
    for (at, kind) in events {
        match kind {
            "introduced" if *version >= at => affected = true,
            "fixed" if *version >= at => affected = false,
            "last_affected" if *version > at => affected = false,
            _ => {}
        }
    }
    Some(affected)
}

fn entry(entry: &Value, tag: &str, version: &Version) -> Verdict {
    let listed = entry
        .get("versions")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    if listed
        .iter()
        .filter_map(Value::as_str)
        .any(|known| known == tag || parse(known).as_ref() == Some(version))
    {
        return Verdict::Affected;
    }
    let verdicts = entry
        .get("ranges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|range| {
            matches!(
                range.get("type").and_then(Value::as_str),
                Some("SEMVER" | "ECOSYSTEM")
            )
        })
        .map(|range| in_range(range, version))
        .collect::<Vec<_>>();
    if verdicts.contains(&Some(true)) {
        Verdict::Affected
    } else if verdicts.contains(&None) {
        Verdict::Unknown
    } else if !verdicts.is_empty() || !listed.is_empty() {
        // Every range was read and excludes the version, or the record only
        // enumerates the versions it affects and this is not one.
        Verdict::Clear
    } else {
        Verdict::Unknown
    }
}

fn record_verdict(
    record: &Value,
    selection: &Selection<'_>,
    tag: &str,
    version: Option<&Version>,
) -> Verdict {
    let Some(version) = version else {
        return Verdict::Unknown;
    };
    let verdicts = own_entries(record, selection)
        .into_iter()
        .map(|own| entry(own, tag, version))
        .collect::<Vec<_>>();
    if verdicts.contains(&Verdict::Affected) {
        Verdict::Affected
    } else if verdicts.is_empty() || verdicts.contains(&Verdict::Unknown) {
        Verdict::Unknown
    } else {
        Verdict::Clear
    }
}

/// The records `tag` is not cleared of, and the ids of those among them whose
/// ranges could not be compared.
pub(super) struct Narrowed {
    pub kept: Vec<Value>,
    pub unmatched: Vec<String>,
}

pub(super) fn narrow(records: Vec<Value>, selection: &Selection<'_>, tag: &str) -> Narrowed {
    let version = asked(tag);
    let mut narrowed = Narrowed {
        kept: Vec::new(),
        unmatched: Vec::new(),
    };
    for record in records {
        match record_verdict(&record, selection, tag, version.as_ref()) {
            Verdict::Clear => {}
            Verdict::Affected => narrowed.kept.push(record),
            Verdict::Unknown => {
                if let Some(id) = record.get("id").and_then(Value::as_str) {
                    narrowed.unmatched.push(id.to_owned());
                }
                narrowed.kept.push(record);
            }
        }
    }
    narrowed
}

#[cfg(test)]
#[path = "versions_tests.rs"]
mod tests;
