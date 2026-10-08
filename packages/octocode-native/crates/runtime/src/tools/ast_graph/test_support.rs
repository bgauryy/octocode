//! Fixtures the astTopology test modules share.
use super::*;
use serde_json::Value;
use std::path::Path;

pub(super) fn run(query: Value, root: &Path) -> AstGraphResult {
    let (paths, security) = crate::tools::test_support::simple_policy(root);
    let parsed: AstTopologyQuery = serde_json::from_value(query).expect("query");
    execute_topology(
        &parsed,
        &paths,
        &security,
        &crate::tools::cancel::NeverCancel,
        None,
    )
}

/// Run a successful topology `query` (research fields added; `path`
/// defaults to `root`) under `root`.
pub(super) fn topology(root: &Path, mut query: Value) -> Value {
    if query.get("path").is_none() {
        query["path"] = root.to_string_lossy().into();
    }
    query["mainGoal"] = "test".into();
    query["reasoning"] = "test".into();
    run(query, root).expect("topology result")
}

/// Write each `(path, text)` under `root`, creating parent directories.
pub(super) fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }
}

/// Write `text` to `file` under `root`, creating parent directories.
pub(super) fn write_file(root: &Path, file: &str, text: &str) {
    write_files(root, &[(file, text)]);
}

/// The scanned-file set of a fixture.
pub(super) fn known(files: &[&str]) -> std::collections::BTreeSet<String> {
    files.iter().map(|file| (*file).to_owned()).collect()
}

/// A temporary directory holding `files`.
pub(super) fn fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
    let temp = tempfile::TempDir::new().expect("temp");
    write_files(temp.path(), files);
    temp
}

/// A Cargo package manifest for a crate named `app`.
pub(super) const CARGO_APP: (&str, &str) = (
    "Cargo.toml",
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
);

/// The `file` of every result row.
pub(super) fn result_files(out: &Value) -> Vec<&str> {
    out["results"]
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["file"].as_str())
        .collect()
}
