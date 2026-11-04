//! Ingest's workspace roots and what the allowed roots refuse.

use super::*;

#[test]
fn default_workspace_stops_at_the_allowed_roots() {
    let dir = fixture();
    let sub = dir.0.as_path().join("app");
    assert_eq!(
        default_workspace(sub.clone(), &workspace_policy(dir.0.as_path())),
        dir.0.as_path()
    );
    // A repository root above the allowed root would be denied: use cwd.
    assert_eq!(default_workspace(sub.clone(), &workspace_policy(&sub)), sub);
}

/// The manifest root is outside the graph digest, so a query checks it
/// against the allowed roots before reading, hashing or scanning anything
/// under it.
#[test]
fn a_manifest_root_outside_the_allowed_roots_is_refused() {
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    let snapshot = PathBuf::from(out.value["dir"].as_str().expect("dir"));
    let outside = crate::tools::test_support::Fixture::new();
    outside.write("src/a.ts", "export const a = 1;\n");
    let manifest_path = snapshot.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("manifest")).expect("json");
    manifest["root"] = json!(std::fs::canonicalize(outside.0.as_path()).expect("real"));
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).expect("json")).expect("tamper");
    for op in ["stale", "issues", "stats"] {
        let refused = ask(dir.0.as_path(), op, None, |_| {});
        assert_eq!(
            refused.value["errorCode"], "outsideAllowedRoots",
            "{op}: {}",
            refused.value
        );
        assert!(refused.value.get("results").is_none(), "{op}");
    }
}

/// A workspace outside the allowed roots is refused with the shared
/// path-policy code, as the scan root is.
#[test]
fn ingest_denials_share_the_path_policy_codes() {
    let dir = fixture();
    let outside = crate::tools::test_support::Fixture::new();
    let denied = ingest(
        &IngestOptions {
            path: dir.0.as_path().join("app"),
            workspace: Some(outside.0.as_path().to_path_buf()),
            ..Default::default()
        },
        &workspace_policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(
        denied.value["errorCode"], "outsideAllowedRoots",
        "{}",
        denied.value
    );
    let scan = ingest(
        &IngestOptions {
            path: outside.0.as_path().to_path_buf(),
            workspace: Some(dir.0.as_path().to_path_buf()),
            ..Default::default()
        },
        &workspace_policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(
        scan.value["errorCode"], "outsideAllowedRoots",
        "{}",
        scan.value
    );
}
