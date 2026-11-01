//! Public graph tools: workspace ownership, exact snapshot selection and typed pages.
use super::*;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::{outcome::DomainResult, result::Continuation};

fn text(row: &Value, key: &str) -> Option<String> {
    row[key].as_str().map(str::to_owned)
}

fn strings(row: &Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn failure(message: impl Into<String>) -> GraphOutput {
    GraphOutput::error(ErrorCode::InvalidInput, message)
}

fn scope(path: &str, paths: &PathPolicy) -> Result<(PathBuf, PathBuf), GraphOutput> {
    let resolve = |path: &str| {
        paths
            .validate(path)
            .map(|p| p.canonical)
            .map_err(|e| GraphOutput::error(e.shared_code(), e.message))
    };
    let workspace = resolve(".")?;
    let root = resolve(path)?;
    if !root.is_dir() || !root.starts_with(&workspace) {
        return Err(failure(
            "path must be a directory inside the current workspace",
        ));
    }
    let home = paths
        .validate_output(workspace.join(".octocode/graph"))
        .map_err(|e| GraphOutput::error(e.shared_code(), e.message))?
        .canonical;
    if !home.starts_with(&workspace) {
        return Err(failure("snapshot storage must remain inside the workspace"));
    }
    Ok((workspace, root))
}

fn missing_snapshot(message: &str, root: &Path) -> GraphOutput {
    let mut out = GraphOutput::error(ErrorCode::NotFound, message);
    out.value["hints"] = json!([
        "Use an existing snapshot ID, or run graphBuild for this path and use its returned ID."
    ]);
    out.value["related"] =
        json!([Continuation::related(ToolId::GraphBuild, json!({"path": root})).build()]);
    out
}

fn exact_snapshot(home: &Path, id: &str, root: &Path) -> Result<String, GraphOutput> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || id.starts_with('.')
        || !is_snapshot(&home.join(id))
    {
        return Err(missing_snapshot(
            "snapshot ID does not exist in this workspace",
            root,
        ));
    }
    Ok(home.join(id).to_string_lossy().into_owned())
}

fn options(row: &Value, paths: &PathPolicy) -> Result<QueryOptions, GraphOutput> {
    let (workspace, root) = scope(row["path"].as_str().unwrap_or("."), paths)?;
    let home = graph_home(Some(&workspace)).map_err(failure)?;
    let id = match row["graph"].as_str() {
        Some(id) => id.to_owned(),
        None => list_snapshots(&home)
            .into_iter()
            .rev()
            .find(|id| {
                read_manifest(&home.join(id))
                    .is_some_and(|m| m["root"] == root.to_string_lossy().as_ref())
            })
            .ok_or_else(|| missing_snapshot("no snapshot for this exact path", &root))?,
    };
    let graph = exact_snapshot(&home, &id, &root)?;
    if read_manifest(Path::new(&graph)).is_none_or(|m| m["root"] != root.to_string_lossy().as_ref())
    {
        return Err(failure(
            "snapshot belongs to a different path; use its original scope",
        ));
    }
    let baseline = row["baseline"]
        .as_str()
        .map(|id| exact_snapshot(&home, id, &root))
        .transpose()?;
    Ok(QueryOptions {
        op: text(row, "operation").unwrap_or_default(),
        path: Some(row["path"].as_str().unwrap_or(".").to_owned()),
        include_provenance: row["includeProvenance"] == true,
        workspace: Some(workspace),
        graph: Some(graph),
        baseline,
        target: text(row, "target").or_else(|| text(row, "cycle")),
        targets: strings(row, "target"),
        reasons: strings(row, "reasons"),
        to: text(row, "to"),
        edges: strings(row, "edges"),
        kind: text(row, "kind"),
        direction: text(row, "direction"),
        depth: row["depth"].as_u64().map(crate::num::u32_of),
        confidence: text(row, "confidence"),
        limit: row["limit"].as_u64().map(crate::num::usize_of),
        offset: row["offset"].as_u64().map(crate::num::usize_of),
        page: row["page"].as_u64().map(crate::num::usize_of),
        list: text(row, "list"),
        view: text(row, "view"),
        changed: strings(row, "changed"),
        since: text(row, "since"),
        detectors: strings(row, "detectors"),
        entries: strings(row, "entries"),
        min_score: row["minScore"].as_f64(),
        min_tier: row["minTier"]
            .as_u64()
            .and_then(|tier| u8::try_from(tier).ok()),
        check_freshness: true,
        ..Default::default()
    })
}

/// Build measurements no follow-up call takes and no coverage claim rests on.
const RECEIPT_INTERNALS: [&str; 7] = [
    "dir",
    "bytes",
    "sha256",
    "extraction",
    "argumentCoverage",
    "buildMs",
    "totalMs",
];

fn build(
    query: &GraphBuildQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    cargo: Option<&str>,
    memory_only: bool,
) -> GraphOutput {
    let (workspace, root) = match scope(&query.path, paths) {
        Ok(scope) => scope,
        Err(error) => return error,
    };
    let options = IngestOptions {
        memory_only,
        workspace: Some(workspace),
        ..IngestOptions::of_query(query, root)
    };
    let mut out = ingest(&options, paths, security, context, cargo);
    if let Some(object) = out.value.as_object_mut() {
        // The scope sits inside the workspace: named relative to it, like every
        // local evidence path.
        if let Some(relative) = object
            .get("root")
            .and_then(Value::as_str)
            .and_then(|path| paths.workspace_relative(path))
        {
            object.insert("root".into(), Value::String(relative));
        }
        // What a follow-up call or a coverage claim needs stays; the rest of
        // the build's measurements live in the snapshot (graphQuery stats).
        for key in RECEIPT_INTERNALS {
            object.remove(key);
        }
        // The manifest keeps every scan fact for reuse checks; the receipt
        // states only the ones that carry information (a cut, an exclude).
        if let Some(scan) = object.get_mut("scan").and_then(Value::as_object_mut) {
            scan.retain(|_, value| {
                !(value == &json!(0)
                    || value == &json!(false)
                    || value.as_array().is_some_and(Vec::is_empty))
            });
        }
    }
    if let Some(id) = out.value["id"].as_str() {
        out.value["related"] = json!([Continuation::related(
            ToolId::GraphQuery,
            json!({"operation":"stats", "path": query.path.as_str(), "graph": id})
        )
        .build()]);
    }
    out
}

/// One validated graphBuild row: ingest a snapshot of its path.
pub(crate) fn execute_build(
    query: &GraphBuildQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    cargo: Option<&str>,
    memory_only: bool,
) -> Result<DomainResult, ExecutionError> {
    context.check()?;
    let out = build(query, paths, security, context, cargo, memory_only);
    context.check()?;
    Ok(result(out))
}

/// One validated graphQuery row against an exact snapshot of its path.
pub(crate) fn execute_query(
    row: &Value,
    paths: &PathPolicy,
    context: &ExecutionContext,
) -> Result<DomainResult, ExecutionError> {
    context.check()?;
    let mut out = match options(row, paths) {
        Ok(options) => super::query::query_cancellable(
            &QueryOptions {
                response_window: context.response_window,
                reuse_snapshot: true,
                ..options
            },
            paths,
            context,
        ),
        Err(error) => error,
    };
    context.check()?;
    pages::convert(&mut out, row);
    Ok(result(out))
}

fn result(out: GraphOutput) -> DomainResult {
    let status = match out.class {
        ExitClass::Success => None,
        ExitClass::Empty => Some("empty"),
        _ => Some("error"),
    };
    DomainResult::payload(out.value, status)
}

mod pages;
#[cfg(test)]
mod tests;
