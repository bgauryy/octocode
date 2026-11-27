//! The `summary` view: what decides the next step, drawn from the finished
//! row. Every operation builds its whole row first (the `details` view), so
//! nothing here is computed twice and nothing the summary leaves out is lost:
//! the `view` field of the same request returns it whole, and a record the
//! row does not list (GitHub's word) keeps the call that reads it.
use octocode_contract_types::SecurityCheckView;
use serde_json::{Map, Value, json};

/// Row fields only the `details` view states. `sources` and `related` are
/// narrowed, not dropped ([`failed_sources`], [`leads`]); `answer`,
/// `packageName` and `version` stay when they say what no other field does.
pub(super) const DETAIL_FIELDS: [&str; 7] = [
    "advisories",
    "moduleOnly",
    "repository",
    "latestVersion",
    "latestPublishedAt",
    "currentPublishedAt",
    "upgradeKind",
];

/// What a summary needs besides the row itself.
pub(super) struct Target<'a> {
    /// The package name and version the request stated: a row field equal to
    /// one is an echo. `None`: the request stated none (a URL, a range the
    /// registry resolved), so the row's own is kept.
    pub asked_name: Option<&'a str>,
    pub asked_version: Option<&'a str>,
    /// Counted advisories no release of the version's own major fixes.
    pub unfixed_in_major: usize,
    /// A package row's answer clause by clause: the summary states the
    /// clauses no field of its row states, and no `answer` when its fields
    /// state every one. `None`: a row that gives no verdict (a
    /// repository's), whose count is of its own records and only its
    /// sentence says so, keeps its sentence.
    pub answer: Option<&'a super::answer::Answer>,
    /// `upgradeTo` is a prerelease of a stable version: the note that says
    /// no stable release clears the advisories stays.
    pub upgrade_prerelease: bool,
}

/// `row` as `view` states it.
pub(super) fn apply(view: SecurityCheckView, row: Value, target: Target<'_>) -> Value {
    match view {
        SecurityCheckView::Details => details(row),
        SecurityCheckView::Summary => summary(row, target),
    }
}

/// The whole row, minus what its other fields derive: an advisory's OSV link
/// is the page its `id` names.
fn details(mut row: Value) -> Value {
    if let Some(rows) = row.get_mut("advisories").and_then(Value::as_array_mut) {
        super::record::drop_derivable_links(rows);
    }
    row
}

/// `fields` with `view:"details"`, for a call that reads a target whole.
pub(super) fn with_details(mut fields: Map<String, Value>) -> Map<String, Value> {
    fields.insert("view".into(), json!(SecurityCheckView::Details));
    fields
}

/// `fields` carrying `view` into the next page of the same walk; the default
/// view is left unstated, as every emitted call leaves a default.
pub(super) fn carry(view: SecurityCheckView, fields: &mut Map<String, Value>) {
    fields.insert("view".into(), json!(view));
}

fn summary(row: Value, target: Target<'_>) -> Value {
    let Value::Object(mut fields) = row else {
        return row;
    };
    let mut lean = Map::new();
    for lead in ["status", "errorCode", "error", "affected", "upgradeTo"] {
        if let Some(value) = fields.shift_remove(lead) {
            lean.insert(lead.to_owned(), value);
        }
    }
    // The ids the row's advisories state: a read of one of them repeats the
    // details view, which the tool description names.
    let on_row = listed_ids(&fields);
    // A version not evaluated, with no verdict, has no advisory of its own:
    // the package's are not counted as its, and its sentence says what they
    // are. So is a target no source covers (a distribution release OSV does
    // not name, or a version asked with no release): its empty list is no
    // count of 0.
    let not_covered = fields
        .get("sources")
        .and_then(Value::as_array)
        .is_some_and(|sources| {
            !sources.is_empty() && sources.iter().all(|entry| entry["status"] == "notCovered")
        });
    let unevaluated = (fields.get("versionEvaluated") == Some(&Value::Bool(false)) || not_covered)
        && lean.get("affected").is_none();
    if let Some(listed) = fields.get("advisories").and_then(Value::as_array)
        && !unevaluated
    {
        indication(&mut lean, listed, target.unfixed_in_major);
    }
    let echoes = |field: &str, asked: Option<&str>, fields: &Map<String, Value>| {
        asked.is_some() && fields.get(field).and_then(Value::as_str) == asked
    };
    if echoes("packageName", target.asked_name, &fields) {
        fields.shift_remove("packageName");
    }
    if echoes("version", target.asked_version, &fields) {
        fields.shift_remove("version");
    }
    // An outdated version names its newer release: the version to move to
    // is the decision, not one call away. The release is stated unless it is
    // the `upgradeTo` already stated, and `outdated:true` is then derived
    // from either naming it; `outdated:false` stays, since an absent field
    // also means the versions could not be compared.
    // The note stays where it is the only reason a row that is affected names
    // no upgrade, and where the upgrade is a prerelease (no stable release
    // clears the advisories); with any other `upgradeTo` the row's own fields
    // say the rest.
    let no_upgrade =
        lean.get("affected") == Some(&Value::Bool(true)) && lean.get("upgradeTo").is_none();
    if !(no_upgrade || target.upgrade_prerelease) {
        fields.shift_remove("upgradeNote");
    }
    let outdated = fields.get("outdated") == Some(&Value::Bool(true));
    let same_as_upgrade = fields.get("latestVersion").is_some()
        && fields.get("latestVersion") == lean.get("upgradeTo");
    for detail in DETAIL_FIELDS {
        if !(outdated && detail == "latestVersion" && !same_as_upgrade) {
            fields.shift_remove(detail);
        }
    }
    if outdated {
        fields.shift_remove("outdated");
    }
    let sources = fields.shift_remove("sources");
    let related = fields.shift_remove("related");
    // The sentence keeps what no field of the summary row says, in its
    // place.
    let sentence = target.answer.map(|answer| {
        let mut stated = lean.clone();
        stated.extend(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        answer.unstated(&stated)
    });
    match sentence {
        Some(Some(sentence)) => {
            fields.insert("answer".to_owned(), json!(sentence));
        }
        Some(None) => {
            fields.shift_remove("answer");
        }
        None => {}
    }
    lean.extend(fields);
    if let Some(failed) = sources.as_ref().and_then(failed_sources) {
        lean.insert("sources".to_owned(), failed);
    }
    // The details view is one `view` value away and named in the tool
    // description: the leads are the repairs and pinned-dependency checks the
    // row itself names, and the reads of records it does not list.
    let leads = leads(related, &on_row);
    if !leads.as_array().is_some_and(Vec::is_empty) {
        lean.insert("related".to_owned(), leads);
    }
    Value::Object(lean)
}

/// The advisory read's summary. Its rows are already one per advisory
/// ([`super::record::group_summary`]) and its `answer` states every range
/// and fix, so only the sources and leads are narrowed here.
pub(super) fn advisory(row: Value) -> Value {
    let Value::Object(mut fields) = row else {
        return row;
    };
    let sources = fields.shift_remove("sources");
    // A record the row does not list (an alias OSV could not answer) keeps
    // the call that reads it, as a package row's summary keeps it.
    let on_row = listed_ids(&fields);
    let leads = leads(fields.shift_remove("related"), &on_row);
    if leads.as_array().is_some_and(|calls| !calls.is_empty()) {
        fields.insert("related".to_owned(), leads);
    }
    if let Some(failed) = sources.as_ref().and_then(failed_sources) {
        fields.insert("sources".to_owned(), failed);
    }
    Value::Object(fields)
}

/// `advisoryCount`, `advisories`, `highestSeverity`, `unfixedInMajorCount`
/// and `link` of a finished list: a withdrawn advisory is listed and never
/// counted. Each counted advisory is one brief row, once, in the list's
/// order (most severe first): its id, CVEs, severity and fixed versions,
/// what decides an upgrade; an `advisory` call reads one whole.
fn indication(lean: &mut Map<String, Value>, listed: &[Value], unfixed_in_major: usize) {
    let counted = listed
        .iter()
        .filter(|row| row.get("withdrawn").is_none())
        .collect::<Vec<_>>();
    lean.insert("advisoryCount".to_owned(), json!(counted.len()));
    let mut rows: Vec<Value> = Vec::new();
    for row in &counted {
        if !rows.iter().any(|known| known["id"] == row["id"]) {
            rows.push((*row).clone());
        }
    }
    if !rows.is_empty() {
        super::record::drop_derivable_links(&mut rows);
        lean.insert("advisories".to_owned(), Value::Array(rows));
    }
    // The first of the most severe: rows keep the order their list gave them.
    let Some(worst) = counted
        .iter()
        .min_by_key(|row| super::record::severity_rank(row))
    else {
        return;
    };
    if let Some(severity) = super::record::ranked_severity(worst) {
        lean.insert("highestSeverity".to_owned(), json!(severity));
    }
    if unfixed_in_major > 0 {
        lean.insert("unfixedInMajorCount".to_owned(), json!(unfixed_in_major));
    }
    // The OSV page of the first advisory is its id's: the output description
    // states that form once.
    let derivable = match (
        lean.get("advisories").and_then(|rows| rows.get(0)),
        worst.get("link").and_then(Value::as_str),
    ) {
        (Some(first), Some(link)) => first
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| super::record::is_derivable_link(id, link)),
        _ => false,
    };
    if !derivable && let Some(link) = worst.get("link").or_else(|| worst.get("source")) {
        lean.insert("link".to_owned(), link.clone());
    }
}

/// The `sources[]` entries that leave the row incomplete: every status but
/// `found` and `noneKnown`, and one of those that names a read it could not
/// make, with why. `None`: every source answered whole.
fn failed_sources(sources: &Value) -> Option<Value> {
    let failed = sources
        .as_array()?
        .iter()
        .filter(|entry| {
            !matches!(entry["status"].as_str(), Some("found" | "noneKnown"))
                || entry.get("errorCode").is_some()
                || entry.get("why").is_some()
        })
        .map(|entry| {
            let mut stated = Map::new();
            for field in ["source", "status", "errorCode", "why"] {
                if let Some(value) = entry.get(field) {
                    stated.insert(field.to_owned(), value.clone());
                }
            }
            Value::Object(stated)
        })
        .collect::<Vec<_>>();
    (!failed.is_empty()).then_some(Value::Array(failed))
}

/// Every id the row's advisories state: each one's id, aliases and CVEs.
fn listed_ids(fields: &Map<String, Value>) -> Vec<String> {
    let listed = fields
        .get("advisories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    listed
        .flat_map(|row| {
            std::iter::once(&row["id"])
                .chain(row["aliases"].as_array().into_iter().flatten())
                .chain(row["cves"].as_array().into_iter().flatten())
        })
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The summary's leads: the package checks the row names, exactly pinned
/// dependencies whose advisories no field of this row counts, or a missing
/// version's nearest published ones; and the advisory reads of records the
/// row does not list (`on_row`: its advisories' ids, aliases and CVEs), such
/// as one only GitHub matches, which its warnings name.
/// Chosen by id: a read of a listed advisory repeats the details view.
fn leads(related: Option<Value>, on_row: &[String]) -> Value {
    related
        .and_then(|related| match related {
            Value::Array(calls) => Some(calls),
            _ => None,
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|call| {
            call.pointer("/input/queries")
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    !rows.is_empty()
                        && rows.iter().all(|row| {
                            row["operation"] == "package"
                                || (row["operation"] == "advisory"
                                    && row["id"]
                                        .as_str()
                                        .is_some_and(|id| !on_row.iter().any(|known| known == id)))
                        })
                })
        })
        .collect()
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
