//! An empty rule whose relation looked only at its neighbor: `has`, `inside`,
//! `precedes` and `follows` stop at the first node they reach unless `stopBy`
//! says otherwise, so `{kind:"class_declaration",has:{kind:"return_statement"}}`
//! reads the class's direct children. When the same rule holds matches with
//! `stopBy:"end"`, the row counts them and gives that call.
use super::exact_miss::{rule_scan, with_rule};
use super::query::MatchQuery;
use super::rows::push_diagnostic;
use super::types::Scope;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use serde_json::{Value, json};

const RELATIONS: [&str; 4] = ["has", "inside", "precedes", "follows"];

/// `rule` with every relation that names no `stopBy` reading to the file's
/// edge; whether any changed. A `not` stays as written: a wider relation there
/// narrows the rule, and this asks what the rule misses.
fn widened(rule: &mut Value) -> bool {
    let mut changed = false;
    for relation in RELATIONS {
        if let Some(inner) = rule.get_mut(relation).filter(|inner| inner.is_object()) {
            if inner.get("stopBy").is_none() {
                inner["stopBy"] = json!("end");
                changed = true;
            }
            changed |= widened(inner);
        }
    }
    for list in ["all", "any"] {
        if let Some(rules) = rule.get_mut(list).and_then(Value::as_array_mut) {
            for inner in rules {
                changed |= widened(inner);
            }
        }
    }
    changed
}

/// Count the matches of `q`'s rule with its relations widened, and attach the
/// note and the call that lists them; `false` when it has none to widen or none
/// match, leaving the row to its other leads.
pub(super) fn attach_relation_miss(
    q: MatchQuery<'_>,
    scope: &Scope,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    out: &mut Value,
) -> bool {
    let Some(mut rule) = q.rule() else {
        return false;
    };
    if !widened(&mut rule) {
        return false;
    }
    let Some(query) = with_rule(q, scope, rule) else {
        return false;
    };
    let Some(scan) = rule_scan(&query, paths, cancel) else {
        return false;
    };
    let (mut count, mut files) = (0, 0);
    for file in scan.files.iter().filter(|file| !file.matches.is_empty()) {
        count += file.matches.len();
        files += 1;
    }
    if count == 0 {
        return false;
    }
    push_diagnostic(
        out,
        json!({
            "code": "structural.rule.neighbor",
            "severity": "warning",
            "stage": "match",
            "message": format!(
                "A relation with no stopBy reads only the immediate neighbor: with stopBy \"end\", this rule holds {count} match(es) in {files} file(s)."
            ),
            "recovery": "broadenSearch runs it with stopBy \"end\".",
        }),
    );
    out["calls"]["broadenSearch"] =
        crate::response::result::Continuation::related_query(query).build();
    true
}
