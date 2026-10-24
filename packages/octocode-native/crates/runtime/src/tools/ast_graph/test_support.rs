//! Fixtures the code-graph test modules share: a temporary tree, ingested
//! into its own workspace and asked through `graph query`.
use super::store::{GraphOutput, IngestOptions, QueryOptions};
use crate::tools::test_support::Fixture;
use serde_json::Value;
use std::path::Path;

/// Write each `(path, text)` under `dir` of `root` (`""`: the root itself).
pub(super) fn write_under(root: &Fixture, dir: &str, files: &[(&str, &str)]) {
    for (path, text) in files {
        let relative = if dir.is_empty() {
            (*path).to_owned()
        } else {
            format!("{dir}/{path}")
        };
        root.write(&relative, text);
    }
}

/// The scanned-file set of a fixture.
pub(super) fn known(files: &[&str]) -> std::collections::BTreeSet<String> {
    files.iter().map(|file| (*file).to_owned()).collect()
}

/// A temporary directory holding `files`.
pub(super) fn fixture(files: &[(&str, &str)]) -> Fixture {
    let temp = Fixture::new();
    write_under(&temp, "", files);
    temp
}

/// A Cargo package manifest for a crate named `app`.
pub(super) const CARGO_APP: (&str, &str) = (
    "Cargo.toml",
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
);

/// Ingests `scan` into the snapshots of `workspace` (the path policy's
/// root), as `octocode graph ingest` does.
pub(super) fn ingest(workspace: &Path, scan: &Path) -> GraphOutput {
    super::store::ingest(
        &IngestOptions {
            path: scan.to_path_buf(),
            workspace: Some(workspace.to_path_buf()),
            force: true,
            ..Default::default()
        },
        &crate::tools::test_support::workspace_policy(workspace),
        &crate::security::ContentSecurity::new(),
        &crate::execution::cancel::NeverCancel,
        None,
    )
}

/// One `graph query` against the latest snapshot of `workspace`.
pub(super) fn ask(workspace: &Path, op: &str, target: Option<&str>) -> Value {
    ask_with(workspace, op, target, |_| {})
}

pub(super) fn ask_with(
    workspace: &Path,
    op: &str,
    target: Option<&str>,
    tweak: impl FnOnce(&mut QueryOptions),
) -> Value {
    let mut options = QueryOptions {
        op: op.into(),
        target: target.map(str::to_owned),
        workspace: Some(workspace.to_path_buf()),
        limit: Some(1000),
        ..Default::default()
    };
    tweak(&mut options);
    super::store::query(
        &options,
        &crate::tools::test_support::workspace_policy(workspace),
    )
    .value
}

/// The `id` of every result row.
pub(super) fn result_ids(out: &Value) -> Vec<&str> {
    out["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["id"].as_str())
        .collect()
}

/// The result row whose `id` is `id`.
pub(super) fn result_row<'v>(out: &'v Value, id: &str) -> &'v Value {
    out["results"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no {id} row: {out}"))
}

/// The member lists of `graph query cycles`.
pub(super) fn cycle_members(out: &Value) -> Vec<Value> {
    out["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["nodes"].clone())
        .collect()
}
