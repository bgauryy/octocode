//! The `summary` view: what decides the next step, drawn from the finished
//! row. Every operation builds its whole row first (the `details` view), so
//! nothing here is computed twice and nothing the summary leaves out is lost:
//! the row's one `related` call asks the same target with `view:"details"`.
use octocode_contract_types::SecurityCheckView;
use serde_json::{Map, Value, json};

/// Row fields only the `details` view states. `sources` and `related` are
/// narrowed, not dropped ([`failed_sources`], [`leads`]); `answer`,
/// `packageName` and `version` stay when they say what no other field does.
pub(super) const DETAIL_FIELDS: [&str; 9] = [
    "upgradeNote",
    "advisories",
    "moduleOnly",
    "repository",
    "latestVersion",
    "latestPublishedAt",
    "currentPublishedAt",
    "isLatest",
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
    /// The operation gives a verdict (`affected`): a row without one keeps
    /// its `answer`, which says why.
    pub gives_verdict: bool,
}

/// `row` as `view` states it.
pub(super) fn apply(view: SecurityCheckView, row: Value, target: Target<'_>) -> Value {
    match view {
        SecurityCheckView::Details => row,
        SecurityCheckView::Summary => summary(row, target),
    }
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
    if let Some(listed) = fields.get("advisories").and_then(Value::as_array) {
        indication(&mut lean, listed, target.unfixed_in_major);
    }
    // The sentence stays where no field says what it does: a row that owes a
    // verdict and gives none (a finished empty list too: a count of 0 there
    // proves nothing, and only the sentence says so), or one with no
    // finished list.
    let no_verdict = (target.gives_verdict && lean.get("affected").is_none())
        || fields.get("advisories").is_none();
    let echoes = |field: &str, asked: Option<&str>, fields: &Map<String, Value>| {
        asked.is_some() && fields.get(field).and_then(Value::as_str) == asked
    };
    if !no_verdict {
        fields.shift_remove("answer");
    }
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
    if let Some(Value::Array(warnings)) = fields.get_mut("warnings") {
        super::global::collapse_withdrawn(warnings);
    }
    let sources = fields.shift_remove("sources");
    let related = fields.shift_remove("related");
    lean.extend(fields);
    if let Some(failed) = sources.as_ref().and_then(failed_sources) {
        lean.insert("sources".to_owned(), failed);
    }
    // The details view is one `view` value away and named in the tool
    // description: the leads are the repairs and pinned-dependency checks the
    // row itself names.
    let leads = leads(related);
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
    fields.shift_remove("related");
    if let Some(failed) = sources.as_ref().and_then(failed_sources) {
        fields.insert("sources".to_owned(), failed);
    }
    Value::Object(fields)
}

/// `advisoryCount`, `highestSeverity`, `unfixedInMajorCount` and `link` of a
/// finished list: a withdrawn advisory is listed and never counted.
fn indication(lean: &mut Map<String, Value>, listed: &[Value], unfixed_in_major: usize) {
    let counted = listed
        .iter()
        .filter(|row| row.get("withdrawn").is_none())
        .collect::<Vec<_>>();
    lean.insert("advisoryCount".to_owned(), json!(counted.len()));
    // The first of the most severe: rows keep the order their list gave them.
    let Some(worst) = counted
        .iter()
        .min_by_key(|row| super::record::severity_rank(row))
    else {
        return;
    };
    if let Some(severity) = worst.get("severity") {
        lean.insert("highestSeverity".to_owned(), severity.clone());
    }
    if unfixed_in_major > 0 {
        lean.insert("unfixedInMajorCount".to_owned(), json!(unfixed_in_major));
    }
    if let Some(link) = worst.get("link").or_else(|| worst.get("source")) {
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

/// The summary's leads: the package checks the row names, exactly pinned
/// dependencies whose advisories no field of this row counts, or a missing
/// version's nearest published ones.
fn leads(related: Option<Value>) -> Value {
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
                    !rows.is_empty() && rows.iter().all(|row| row["operation"] == "package")
                })
        })
        .collect()
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
