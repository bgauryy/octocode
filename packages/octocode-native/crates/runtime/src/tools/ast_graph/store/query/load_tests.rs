//! Choosing and loading a snapshot under the path policy.
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::policy::path::PathPolicyConfig;
use crate::security::ContentSecurity;
use crate::tools::ast_graph::store::{IngestOptions, configured_graph_home, ingest};
use crate::tools::test_support::{Fixture, workspace_policy};

/// A workspace with one source and a configured snapshot home outside it,
/// built once: the policy reads the workspace and writes the home.
fn built_in_configured_home() -> (Fixture, tempfile::TempDir, PathPolicy, PathBuf) {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = () => 1;\n");
    let home = tempfile::tempdir().expect("home");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(dir.0.to_path_buf()),
        output_roots: vec![configured_graph_home(home.path())],
        ..Default::default()
    })
    .expect("policy");
    let out = ingest(
        &IngestOptions {
            path: dir.0.join("app"),
            workspace: Some(dir.0.to_path_buf()),
            home: Some(home.path().to_path_buf()),
            ..Default::default()
        },
        &paths,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let snapshot = PathBuf::from(out.value["dir"].as_str().expect("dir"));
    (dir, home, paths, snapshot)
}

fn options(dir: &Fixture, home: &Path, graph: Option<String>) -> QueryOptions {
    QueryOptions {
        op: "stats".into(),
        workspace: Some(dir.0.to_path_buf()),
        home: Some(home.to_path_buf()),
        graph,
        ..Default::default()
    }
}

/// A snapshot lives in an output root, and its manifest's
/// `root` is data. A root that names that output root (or anything outside
/// the read roots) is refused before any read runs under it, at `load`
/// itself, whatever the tool in front of it checks.
#[test]
fn a_manifest_root_in_an_output_root_is_refused_at_load() {
    let (dir, home, paths, snapshot) = built_in_configured_home();
    let fine = load(&options(&dir, home.path(), None), &paths, &NeverCancel);
    assert!(fine.is_ok(), "the built snapshot loads");
    let manifest_path = snapshot.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("manifest")).expect("json");
    let output_root = std::fs::canonicalize(configured_graph_home(home.path())).expect("home");
    manifest["root"] = json!(output_root.to_string_lossy());
    std::fs::write(&manifest_path, manifest.to_string()).expect("crafted manifest");
    let refused = load(&options(&dir, home.path(), None), &paths, &NeverCancel)
        .err()
        .expect("refused");
    assert_eq!(
        refused.value["errorCode"], "outsideAllowedRoots",
        "{}",
        refused.value
    );
}

/// An explicit `graph` path outside every root is looked at by no file
/// check: an existing file and a missing one get the same answer, so the
/// query tells nothing about what lies outside.
#[test]
fn an_explicit_graph_path_outside_the_roots_reveals_nothing() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = () => 1;\n");
    let out = ingest(
        &IngestOptions {
            path: dir.0.join("app"),
            workspace: Some(dir.0.to_path_buf()),
            ..Default::default()
        },
        &workspace_policy(&dir.0),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let outside = tempfile::tempdir().expect("outside");
    let present = outside.path().join("present.txt");
    std::fs::write(&present, "x").expect("present");
    let absent = outside.path().join("absent.txt");
    let answer = |path: &Path| {
        let options = QueryOptions {
            op: "stats".into(),
            workspace: Some(dir.0.to_path_buf()),
            graph: Some(path.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let failure = load(&options, &workspace_policy(&dir.0), &NeverCancel)
            .err()
            .expect("no snapshot there");
        (
            failure.value["errorCode"].clone(),
            failure.value["error"]
                .as_str()
                .unwrap_or_default()
                .replace(&*path.to_string_lossy(), "<graph>"),
        )
    };
    assert_eq!(answer(&present), answer(&absent));
    assert_eq!(answer(&present).0, "notFound");
}
