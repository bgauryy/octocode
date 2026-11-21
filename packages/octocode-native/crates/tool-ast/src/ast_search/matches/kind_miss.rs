//! An empty rule that names a kind and a regex, where the text is another kind's:
//! in Rust, `Result` in a signature is a `type_identifier`, never an `identifier`.
//! When the rule holds matches without its kind, the row names the kinds it saw,
//! counts the nodes and gives the call that lists them.
use super::exact_miss::{asked_view, lead_call, rule_scan, with_rule};
use super::query::MatchQuery;
use super::reads::row_file;
use super::rows::push_diagnostic;
use super::types::{ScannedFile, Scope};
use octocode_contract_types::AstSearchQueryMatch0;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// `rule` without its kind, innermost nodes only: an unanchored regex also holds
/// in every node around the text. `None` for a rule naming no kind and regex, or
/// with a `not` of its own.
fn without_kind(rule: &Value) -> Option<(String, Value)> {
    let asked = rule.get("kind")?.as_str()?.to_owned();
    let regex = rule.get("regex")?.clone();
    if rule.get("not").is_some() {
        return None;
    }
    let mut wider = rule.clone();
    let fields = wider.as_object_mut()?;
    fields.remove("kind");
    fields.insert(
        "not".into(),
        json!({"has": {"regex": regex, "stopBy": "end"}}),
    );
    Some((asked, wider))
}

/// The node kinds of `file`'s matches. A directory scan keeps only row prices,
/// so the file is matched once more for them.
fn kinds_in(
    file: &ScannedFile,
    query: &AstSearchQueryMatch0,
    scope: &Scope,
) -> Option<BTreeSet<String>> {
    let kinds = |found: &[octocode_engine::structural::StructuralDetailedMatch]| {
        found
            .iter()
            .filter_map(|found| found.node_kind.clone())
            .collect::<BTreeSet<_>>()
    };
    if !file.matches.held().is_empty() {
        return Some(kinds(file.matches.held()));
    }
    let path = row_file(scope, &json!({"path": file.path}))?;
    let bytes = scope.parse_source(&path).ok()??;
    let source = super::super::source::matched_text(&path, &bytes).ok()?;
    let parser = Some(file.parser.as_str()).filter(|parser| !parser.is_empty());
    let found = octocode_engine::structural::search_detailed(
        &source,
        &path.to_string_lossy(),
        parser,
        None,
        MatchQuery::Content(query).rule().as_ref(),
    );
    Some(kinds(&found.matches))
}

/// Count the nodes `q`'s rule holds without its kind, and attach the note and the
/// call that lists them; `false` when it names no kind or none match, leaving the
/// row to its other leads.
pub(super) fn attach_kind_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    out: &mut Value,
) -> bool {
    let Some((asked, rule)) = q.rule().as_ref().and_then(without_kind) else {
        return false;
    };
    let Some(query) = with_rule(q, scope, rule) else {
        return false;
    };
    let Some(scan) = rule_scan(&query, paths, cancel, super::scan::Keeps::Counts) else {
        return false;
    };
    let matched = || scan.files.iter().filter(|file| !file.matches.is_empty());
    let count = matched().map(|file| file.matches.len()).sum::<usize>();
    let Some(first) = matched().next() else {
        return false;
    };
    let seen = kinds_in(first, &query, scope)
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");
    let files = matched().count();
    push_diagnostic(
        out,
        json!({
            "code": "structural.rule.otherKind",
            "severity": "warning",
            "stage": "match",
            "message": format!(
                "No {asked} node holds this regex, but {count} node(s) of another kind in {files} file(s) do, such as {seen}."
            ),
        }),
    );
    // The diagnostic counts matches this rule leaves out: the row needs the
    // rule that returns them.
    out["calls"]["broadenSearch"] = lead_call(query, asked_view(q).as_ref()).required().build();
    true
}
