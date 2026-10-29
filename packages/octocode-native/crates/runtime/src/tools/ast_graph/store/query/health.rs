//! Snapshot health: diagnostics, unlinked imports, staleness and the
//! re-ingest command.
use super::*;

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

/// The ingest that rebuilds the snapshot of `manifest` with its own scan
/// options; a scan cut at maxFiles re-ingests with twice the bound, the only
/// limit a re-ingest can lift.
pub(super) fn reingest_command(manifest: &Value) -> String {
    let scan = &manifest["scan"];
    let root = manifest["root"].as_str().unwrap_or_default();
    let mut command = format!("octocode graph ingest {}", quote(root));
    let max_files = match scan["maxFiles"].as_u64() {
        Some(max) if scan["truncated"] == true => {
            Some(max.saturating_mul(2).min(u64::from(u32::MAX)))
        }
        max => {
            max.filter(|max| *max != u64::from(crate::tools::ast_graph::limits::LIMITS.max_files))
        }
    };
    if let Some(max) = max_files {
        command.push_str(&format!(" --max-files {max}"));
    }
    for dir in scan["excludeDir"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        command.push_str(&format!(" --exclude {}", quote(dir)));
    }
    for (language, globs) in scan["languageGlobs"].as_object().into_iter().flatten() {
        for glob in globs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            command.push_str(&format!(
                " --language-glob {}",
                quote(&format!("{language}={glob}"))
            ));
        }
    }
    command
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
    extra.insert(
        "fresh".into(),
        json!(rows.is_empty() && freshness.added_checked && analysis_current),
    );
    let scan = &g.manifest["scan"];
    let max_files = scan["maxFiles"].as_u64();
    if !freshness.added_checked {
        extra.insert(
            "unchecked".into(),
            json!(format!(
                "files added since ingest: the snapshot's scan stopped at maxFiles ({})",
                max_files.map_or_else(|| "?".to_owned(), |max| max.to_string())
            )),
        );
    }
    // A scan cut at maxFiles names its widened re-ingest in the graph block.
    if (!rows.is_empty() || !analysis_current) && freshness.added_checked {
        extra.insert("reingest".into(), json!(reingest_command(&g.manifest)));
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
