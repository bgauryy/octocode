//! Snapshot health: diagnostics, unlinked imports, staleness and the
//! re-ingest call.
use super::*;
use crate::id::ToolId;
use crate::response::result::Continuation;

pub(super) fn diagnostics(g: &Graph, filter: Option<&str>) -> Page {
    let rows =
        g.t.diagnostics
            .iter()
            .filter(|diag| {
                filter.is_none_or(|f| g.t.str(diag.code) == f || g.t.str(diag.file).starts_with(f))
            })
            .map(|diag| {
                let mut row = json!({
                    "file": g.t.str(diag.file),
                    "code": g.t.str(diag.code),
                    "message": g.t.str(diag.message),
                });
                if diag.line != NONE {
                    row["line"] = json!(diag.line);
                }
                if diag.target != NONE {
                    row["target"] = json!(g.key(diag.target));
                }
                row
            })
            .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
        start: 0,
    }
}

/// The unlinked path imports that name `file` (see the ingest's
/// `unlinked-path-import` diagnostics): importers no edge records.
pub(super) fn unlinked_path_imports(g: &Graph, file: u32) -> Vec<Value> {
    g.t.diagnostics
        .iter()
        .filter(|diag| {
            diag.target == file && g.t.str(diag.code) == DiagnosticCode::UnlinkedPathImport.as_str()
        })
        .map(|diag| {
            json!({
                "file": g.t.str(diag.file),
                "line": diag.line,
                "message": g.t.str(diag.message),
            })
        })
        .collect()
}

/// The related graphBuild call that rebuilds the snapshot of `manifest`
/// with its own scan options: its root (workspace-relative), excluded
/// directories and parser globs.
pub(super) fn reingest_call(manifest: &Value) -> Value {
    let scan = &manifest["scan"];
    let root = manifest["root"].as_str().unwrap_or_default();
    let path = manifest["workspace"]
        .as_str()
        .and_then(|workspace| Path::new(root).strip_prefix(workspace).ok())
        .map(|relative| relative.to_string_lossy().into_owned())
        .map_or_else(
            || root.to_owned(),
            |relative| {
                if relative.is_empty() {
                    ".".to_owned()
                } else {
                    relative
                }
            },
        );
    let mut row = Map::from_iter([("path".to_owned(), json!(path))]);
    if let Some(exclude) = scan["excludeDir"]
        .as_array()
        .filter(|dirs| !dirs.is_empty())
    {
        row.insert("exclude".into(), json!(exclude));
    }
    if let Some(globs) = scan["languageGlobs"]
        .as_object()
        .filter(|globs| !globs.is_empty())
    {
        row.insert("parserGlobs".into(), json!(globs));
    }
    Continuation::related(ToolId::GraphBuild, Value::Object(row)).build()
}

/// What changed under the snapshot's root since ingest: sources edited,
/// removed or added, and linking configs (manifests, tsconfig, ...) edited,
/// removed or added; the same check an ingest runs before reusing it.
pub(super) fn stale(g: &Graph, paths: &PathPolicy) -> (Map<String, Value>, Page) {
    let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
    let freshness = super::super::snapshot_changes(paths, &root, &g.t, &g.manifest, false);
    let rows = freshness
        .changes
        .into_iter()
        .map(|change| {
            let mut row = json!({"file": change.file, "status": change.status});
            if change.config {
                row["config"] = json!(true);
            }
            row
        })
        .collect::<Vec<_>>();
    let mut extra = Map::new();
    let analysis_current = super::super::analysis_current(&g.manifest);
    extra.insert("analysisCurrent".into(), json!(analysis_current));
    extra.insert("checked".into(), json!(g.t.digests.len()));
    let fresh = rows.is_empty() && analysis_current;
    extra.insert("fresh".into(), json!(fresh));
    if !fresh {
        extra.insert("reingest".into(), reingest_call(&g.manifest));
    }
    (
        extra,
        Page {
            total: rows.len(),
            rows,
            start: 0,
        },
    )
}
