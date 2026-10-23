use super::{
    common_directory, prefix_relative_paths, relative_continuation_path,
    relativize_continuation_paths, rewrite_paths, visit_paths,
};
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::output::PathAnchor;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Build the public envelope for executed rows; `queries` maps each row
/// position to its validated query (`None` for a rejected row).
///
/// The tool's [`PathAnchor`] decides how file paths are named. Local file
/// paths are anchored on the workspace root: a path inside it is emitted
/// relative to it, so a copied row path resolves as-is, and `root` is that
/// root; a path outside it stays absolute.
pub(crate) fn envelope_in(
    mut rows: Vec<Value>,
    tool: ToolId,
    queries: &[Option<&Value>],
    paths: &PathPolicy,
) -> Value {
    let roots: Vec<Option<PathBuf>> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| {
            if row["status"] == "error" {
                return None;
            }
            let index = row["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(position);
            let path = queries
                .get(index)
                .copied()
                .flatten()?
                .get("path")?
                .as_str()?;
            paths.validate(path).ok().map(|valid| valid.canonical)
        })
        .collect();
    let anchor = tool.output().path_anchor();
    if anchor == PathAnchor::Common {
        return envelope(tool, rows);
    }
    if anchor == PathAnchor::QueryParent {
        for (row, root) in rows.iter_mut().zip(&roots) {
            if let Some(parent) = root.as_deref().and_then(Path::parent) {
                let parent = parent.to_string_lossy();
                prefix_relative_paths(&mut row["data"], 0, &parent);
            }
        }
    }
    let workspace = paths
        .validate(".")
        .ok()
        .map(|valid| valid.canonical.to_string_lossy().into_owned())
        .filter(|root| Path::new(root).parent().is_some());
    if let Some(workspace) = workspace.as_deref() {
        for row in &mut rows {
            relativize_continuation_paths(&mut row["data"], workspace);
        }
    }
    // The checked-out commit is provenance: read only for rows that ask
    // for receipts.
    let receipts = rows
        .iter()
        .enumerate()
        .map(|(position, row)| {
            let index = row["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(position);
            queries.get(index).copied().flatten().is_some_and(|query| {
                super::super::extras::wants(query, octocode_contract_types::ExtraKind::Receipts)
            })
        })
        .collect::<Vec<_>>();
    let heads = checked_out_heads(tool, &roots, &receipts);
    let mut value = compact(tool, rows, workspace);
    attach_heads(&mut value, &heads);
    value
}

/// HEAD of each receipts row's root for the tools that read the working
/// tree directly; empty for every other tool. A root is read once per
/// response.
fn checked_out_heads(
    tool: ToolId,
    roots: &[Option<PathBuf>],
    receipts: &[bool],
) -> Vec<Option<String>> {
    if !tool.output().reads_worktree() || !receipts.contains(&true) {
        return Vec::new();
    }
    let mut seen = std::collections::HashMap::<&Path, Option<String>>::new();
    roots
        .iter()
        .zip(receipts)
        .map(|(root, wanted)| {
            let root = root.as_deref().filter(|_| *wanted)?;
            seen.entry(root)
                .or_insert_with(|| crate::git_head::head_sha(root))
                .clone()
        })
        .collect()
}

/// Report the checked-out commit once: `shared.commitSha` when every row
/// with a root reads the same commit, else `commitSha` on each row that has
/// one (roots in different repositories, or some outside any repository).
fn attach_heads(value: &mut Value, heads: &[Option<String>]) {
    let Some(first) = heads.iter().flatten().next() else {
        return;
    };
    let Some(rows) = value["results"].as_array_mut() else {
        return;
    };
    let shared = rows.iter().zip(heads).all(|(row, head)| {
        head.as_ref() == Some(first) || (head.is_none() && row["status"] == "error")
    });
    if shared {
        let first = first.clone();
        match value.get_mut("shared").and_then(Value::as_object_mut) {
            Some(map) => {
                map.insert("commitSha".into(), json!(first));
            }
            None => value["shared"] = json!({ "commitSha": first }),
        }
        return;
    }
    for (row, head) in rows.iter_mut().zip(heads) {
        if let (Some(head), Some(data)) = (head, row["data"].as_object_mut()) {
            data.insert("commitSha".into(), json!(head));
        }
    }
}

pub fn envelope(tool: ToolId, rows: Vec<Value>) -> Value {
    let mut paths = Vec::new();
    for row in &rows {
        visit_paths(&row["data"], 0, &mut paths);
    }
    let base = common_directory(&paths);
    compact(tool, rows, base)
}

/// Rewrite every absolute row path under `base` relative to it (the base
/// itself becomes `.`), hoist `tool`'s shared leaf fields, and wrap the rows. Paths
/// outside `base` stay absolute; without such a path, or a continuation
/// whose relative `path` resolves against `base`, there is no `base`.
fn compact(tool: ToolId, mut rows: Vec<Value>, base: Option<String>) -> Value {
    let base = base.filter(|base| {
        let inside = format!("{base}/");
        let mut paths = Vec::new();
        for row in &rows {
            visit_paths(&row["data"], 0, &mut paths);
        }
        paths
            .iter()
            .any(|path| path == base || path.starts_with(&inside))
            || rows
                .iter()
                .any(|row| relative_continuation_path(&row["data"]))
    });
    for row in &mut rows {
        rewrite_paths(&mut row["data"], 0, base.as_deref());
    }
    let shared = crate::contracts::shared_fields::hoist(tool, &mut rows);
    let mut value = json!({"results":rows});
    if let Some(base) = base {
        value["root"] = json!(base);
    }
    if let Some(shared) = shared {
        value["shared"] = Value::Object(shared);
    }
    value
}
