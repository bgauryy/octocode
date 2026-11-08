//! An empty `rule.pattern` scan whose pattern parsed as a construct the scope
//! holds none of: a fragment that is part of other code reads alone as
//! something else (TypeScript `$N: number` is a labeled statement, not a
//! class field). The row names what it parsed as and offers the
//! `{context, selector}` form that gives the fragment its surroundings.
use super::exact_miss::{rule_scan, with_rule};
use super::query::MatchQuery;
use super::rows::push_diagnostic;
use super::scope::scope_extension;
use super::types::Scope;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use serde_json::{Value, json};

/// Attach the note when the pattern's kind has no node in a complete scan of
/// the scope; `false` when it does, or when that is unknown.
pub(super) fn attach_construct_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    out: &mut Value,
) -> bool {
    let Some(pattern) = q.pattern_text() else {
        return false;
    };
    let Some(kind) = scope_extension(scope)
        .and_then(|extension| octocode_engine::structural::pattern_kind(&pattern, &extension))
    else {
        return false;
    };
    let Some(query) = with_rule(q, scope, json!({ "kind": kind })) else {
        return false;
    };
    let Some(scan) = rule_scan(&query, paths, cancel) else {
        return false;
    };
    let complete =
        !scan.truncated && scan.skips.1 + scan.skips.2 == 0 && scan.walk_errors.skipped == 0;
    if !complete || scan.files.iter().any(|file| !file.matches.is_empty()) {
        return false;
    }
    push_diagnostic(
        out,
        json!({
            "code": "structural.pattern.construct",
            "severity": "warning",
            "stage": "match",
            "message": format!(
                "The pattern parsed as a {kind} node, and the scope holds none. A fragment of other code reads alone as another construct."
            ),
            "recovery": format!(
                "Give the fragment its surroundings: rule {{\"pattern\":{{\"context\":\"<code around {pattern}>\",\"selector\":\"<kind of the node you mean>\"}}}}."
            ),
        }),
    );
    true
}
