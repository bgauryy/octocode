//! astTopology drift: the diff between a stored graph and the current one.

use super::{page::*, types::*};
use crate::tools::id::query_limits::ast_topology::PAGE_MAXIMUM;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use serde_json::{Map, Value, json};

/// Compare the import topology of a baseline root against the head `path` and
/// report typed structural drift. Builds two independent snapshots and diffs
/// them through the shared engine so results stay AST-only and deterministic.
pub(crate) fn drift(
    q: &AstTopologyQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
    extras: &super::graph::BuildExtras,
) -> AstGraphResult {
    let head = super::graph::build_graph_with(q, paths, security, cancel, extras)?;
    let baseline_root = q
        .baseline()
        .map(str::to_owned)
        .ok_or_else(|| AstGraphError::new("invalidGraphQuery", "drift requires baseline"))?;
    let mut base_query = q.clone();
    base_query.set_path(baseline_root);
    let mut base = super::graph::build_graph_with(&base_query, paths, security, cancel, extras)?;
    cancel
        .check()
        .map_err(|e| AstGraphError::new("ast.cancelled", e))?;

    // The native scanner keys every node on a path relative to its own scan
    // root, so two-root drift shares identity across differing absolute roots.
    // Equalize the root metadata so the engine's root-mismatch gate — which
    // protects consumers that key on absolute paths — does not false-refuse this
    // legitimately comparable pair. The real roots stay visible in `path` /
    // `baseline` below.
    base.code_graph.snapshot.root = head.code_graph.snapshot.root.clone();
    let diff = octocode_engine::graph::diff_graphs(&base.code_graph, &head.code_graph);

    let items = drift_rows(&diff);
    let summary = drift_summary(&diff);

    let mut base_map = Map::new();
    base_map.insert("operation".into(), json!("drift"));
    base_map.insert("path".into(), json!(head.display_path));
    base_map.insert("baseline".into(), json!(base.display_path));
    base_map.insert("filesScanned".into(), json!(head.facts.len()));
    base_map.insert("baselineFilesScanned".into(), json!(base.facts.len()));

    let snapshot = crate::digest::json_sha256(&items);
    if q.diagnostic_snapshot()
        .is_some_and(|expected| expected != snapshot)
    {
        base_map.insert("results".into(), json!([]));
        insert_snapshot_changed(&mut base_map);
        base_map.insert(
            "next".into(),
            json!({"restartDiagnostics": restart_continuation(q)}),
        );
        return Ok(Value::Object(base_map));
    }
    let (page, pagination) = paginate(items, q);
    let has_more = pagination["hasMore"] == json!(true);
    let mut warnings = Vec::new();
    if !diff.comparable {
        base_map.insert("confidence".into(), json!("low"));
        warnings.push("graphs are not comparable — see summary.incompatibilities".to_owned());
    }
    warnings.extend(out_of_range_warning(&pagination));
    base_map.insert("results".into(), Value::Array(page));
    base_map.insert("pagination".into(), pagination);
    base_map.insert("summary".into(), summary);
    if !warnings.is_empty() {
        base_map.insert("warnings".into(), json!(warnings));
    }

    let mut reasons: Vec<&str> = Vec::new();
    if head.truncated || base.truncated {
        reasons.push("maxFiles");
    }
    if head.files_skipped > 0 || base.files_skipped > 0 {
        reasons.push("filesSkipped");
    }
    if !reasons.is_empty() {
        base_map.insert("isPartial".into(), json!(true));
        base_map.insert("partialReasons".into(), json!(reasons));
    }
    let results_state = if has_more {
        "pageable"
    } else if reasons.is_empty() {
        "complete"
    } else {
        "truncated"
    };
    let graph_state =
        if head.truncated || base.truncated || head.files_skipped > 0 || base.files_skipped > 0 {
            "scan-truncated"
        } else {
            "complete"
        };
    let mut completeness = Map::new();
    for (key, state) in [("results", results_state), ("graph", graph_state)] {
        if state != "complete" {
            completeness.insert(key.into(), json!(state));
        }
    }
    if !completeness.is_empty() {
        base_map.insert("completeness".into(), Value::Object(completeness));
    }
    if has_more && (q.page() as usize) < PAGE_MAXIMUM {
        let next = continuation(
            q,
            Some(q.page() + 1),
            None,
            Some(json!(snapshot)),
            "Continue topology drift results.",
        );
        base_map["pagination"]["resultId"] = json!(snapshot);
        base_map.insert("next".into(), json!({ "nextPage": next }));
    }
    Ok(Value::Object(base_map))
}

/// One row per drift change: relations, static/dynamic transitions,
/// cycles and files.
pub(super) fn drift_rows(diff: &octocode_engine::graph::GraphDiff) -> Vec<Value> {
    let mut items: Vec<Value> = Vec::new();
    for rel in &diff.relations.added {
        items.push(json!({"category":"relation","change":"added","from":rel.from.0,"to":rel.to.0,"edgeKind":format!("{:?}",rel.kind),"confidence":"syntactic"}));
    }
    for rel in &diff.relations.removed {
        items.push(json!({"category":"relation","change":"removed","from":rel.from.0,"to":rel.to.0,"edgeKind":format!("{:?}",rel.kind),"confidence":"syntactic"}));
    }
    for rel in &diff.relations.static_to_dynamic {
        items.push(json!({"category":"transition","change":"staticToDynamic","from":rel.from.0,"to":rel.to.0,"confidence":"syntactic"}));
    }
    for rel in &diff.relations.dynamic_to_static {
        items.push(json!({"category":"transition","change":"dynamicToStatic","from":rel.from.0,"to":rel.to.0,"confidence":"syntactic"}));
    }
    for cycle in &diff.cycles.added {
        items.push(
            json!({"category":"cycle","change":"added","files":cycle,"confidence":"syntactic"}),
        );
    }
    for cycle in &diff.cycles.resolved {
        items.push(
            json!({"category":"cycle","change":"resolved","files":cycle,"confidence":"syntactic"}),
        );
    }
    for file in &diff.files.added {
        items.push(json!({"category":"file","change":"added","file":file}));
    }
    for file in &diff.files.removed {
        items.push(json!({"category":"file","change":"removed","file":file}));
    }
    for file in &diff.files.changed {
        items.push(json!({"category":"file","change":"changed","file":file}));
    }
    items
}

pub(super) fn drift_summary(diff: &octocode_engine::graph::GraphDiff) -> Value {
    json!({
        "comparable": diff.comparable,
        "incompatibilities": serde_json::to_value(&diff.incompatibilities).unwrap_or(Value::Null),
        "relationsAdded": diff.relations.added.len(),
        "relationsRemoved": diff.relations.removed.len(),
        "staticToDynamic": diff.relations.static_to_dynamic.len(),
        "dynamicToStatic": diff.relations.dynamic_to_static.len(),
        "cyclesAdded": diff.cycles.added.len(),
        "cyclesResolved": diff.cycles.resolved.len(),
        "filesAdded": diff.files.added.len(),
        "filesRemoved": diff.files.removed.len(),
        "filesChanged": diff.files.changed.len(),
        "completeness": serde_json::to_value(&diff.completeness).unwrap_or(Value::Null),
        "metrics": serde_json::to_value(&diff.metrics).unwrap_or(Value::Null),
    })
}
