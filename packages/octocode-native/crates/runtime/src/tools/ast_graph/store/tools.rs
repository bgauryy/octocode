//! Public graph tools: workspace ownership, exact snapshot selection and typed pages.
use super::*;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::{outcome::DomainResult, result::Continuation};

pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {}

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

fn scope(row: &Value, paths: &PathPolicy) -> Result<(PathBuf, PathBuf), GraphOutput> {
    let resolve = |path: &str| {
        paths
            .validate(path)
            .map(|p| p.canonical)
            .map_err(|e| GraphOutput::error(e.shared_code(), e.message))
    };
    let workspace = resolve(".")?;
    let root = resolve(row["path"].as_str().unwrap_or("."))?;
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

fn exact_snapshot(home: &Path, id: &str) -> Result<String, GraphOutput> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || id.starts_with('.')
        || !is_snapshot(&home.join(id))
    {
        return Err(GraphOutput::error(
            ErrorCode::NotFound,
            "snapshot ID does not exist in this workspace; run graphBuild",
        ));
    }
    Ok(home.join(id).to_string_lossy().into_owned())
}

fn options(row: &Value, paths: &PathPolicy) -> Result<QueryOptions, GraphOutput> {
    let (workspace, root) = scope(row, paths)?;
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
            .ok_or_else(|| {
                GraphOutput::error(
                    ErrorCode::NotFound,
                    "no snapshot for this exact path; run graphBuild",
                )
            })?,
    };
    let graph = exact_snapshot(&home, &id)?;
    if read_manifest(Path::new(&graph)).is_none_or(|m| m["root"] != root.to_string_lossy().as_ref())
    {
        return Err(failure(
            "snapshot belongs to a different path; use its original scope",
        ));
    }
    let baseline = row["baseline"]
        .as_str()
        .map(|id| exact_snapshot(&home, id))
        .transpose()?;
    Ok(QueryOptions {
        op: text(row, "operation").unwrap_or_default(),
        include_provenance: row["includeProvenance"].as_bool(),
        workspace: Some(workspace),
        graph: Some(graph),
        baseline,
        target: text(row, "target").or_else(|| text(row, "cycle")),
        to: text(row, "to"),
        edges: strings(row, "edges"),
        kind: text(row, "kind"),
        direction: text(row, "direction"),
        depth: row["depth"].as_u64().map(|n| n as u32),
        confidence: text(row, "confidence"),
        limit: row["limit"].as_u64().map(|n| n as usize),
        offset: row["offset"].as_u64().map(|n| n as usize),
        list: text(row, "list"),
        changed: strings(row, "changed"),
        since: text(row, "since"),
        check_freshness: true,
        ..Default::default()
    })
}

fn build(
    row: &Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    cargo: Option<&str>,
) -> GraphOutput {
    let (workspace, root) = match scope(row, paths) {
        Ok(scope) => scope,
        Err(error) => return error,
    };
    let language_globs = match serde_json::from_value(
        row.get("parserGlobs").cloned().unwrap_or_else(|| json!({})),
    ) {
        Ok(globs) => globs,
        Err(error) => return failure(error.to_string()),
    };
    let options = IngestOptions {
        path: root,
        workspace: Some(workspace),
        exclude_dir: strings(row, "exclude"),
        max_files: row["maxFiles"].as_u64().map(|n| n as u32),
        force: row["force"] == true,
        keep: row["keep"].as_u64().map(|n| n as usize),
        language_globs,
    };
    let mut out = ingest(&options, paths, security, context, cargo);
    if let Some(object) = out.value.as_object_mut() {
        object.remove("next");
    }
    if let Some(id) = out.value["id"].as_str() {
        out.value["related"] = json!([Continuation::related(
            ToolId::GraphQuery,
            json!({"operation":"stats", "path": options.path, "graph": id})
        )
        .build()]);
    }
    out
}

pub(crate) fn execute(
    tool: ToolId,
    row: &Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    context: &ExecutionContext,
    cargo: Option<&str>,
) -> Result<DomainResult, ExecutionError> {
    context.check()?;
    let mut out = if tool == ToolId::GraphBuild {
        build(row, paths, security, context, cargo)
    } else {
        match options(row, paths) {
            Ok(options) => super::query::query_cancellable(&options, paths, context),
            Err(error) => error,
        }
    };
    context.check()?;
    if tool == ToolId::GraphQuery {
        pages::convert(&mut out.value, row, out.next_query.as_deref());
    }
    let status = match out.class {
        ExitClass::Success => None,
        ExitClass::Empty => Some("empty"),
        _ => Some("error"),
    };
    Ok(DomainResult::payload(out.value, status))
}

mod pages;
#[cfg(test)]
mod tests;
