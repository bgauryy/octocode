use super::*;

/// The graph a build states, without the snapshot id, timings or the digest.
/// The digest also covers each source's file stamp, which is recorded only for a
/// file untouched for `SourceStamp::SETTLE` before the build started: two builds
/// of one graph can differ in digest by wall-clock timing alone.
fn graph_content(build: &GraphOutput) -> Value {
    json!({
        "root": build.value["root"],
        "counts": build.value["counts"],
        "calls": build.value["calls"],
        "argumentCoverage": build.value["argumentCoverage"],
        "linkage": build.value["linkage"],
        "scan": build.value["scan"],
    })
}

#[test]
fn every_build_extracts_unchanged_inputs_without_reusing_a_snapshot() {
    let dir = fixture();
    let first = ingest_fixture(&dir.0, None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    let second = ingest_fixture(&dir.0, None);
    assert_fresh_build(&second);
    assert_ne!(first.value["id"], second.value["id"], "{}", second.value);
    assert_eq!(graph_content(&first), graph_content(&second));
    assert!(first.value["counts"]["nodes"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn changed_sources_and_checkpoint_refresh_preserve_fresh_builds() {
    let dir = fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(first.class, ExitClass::Success);
    let second = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&second);
    assert_ne!(second.value["id"], first.value["id"], "{}", second.value);
    assert_eq!(graph_content(&second), graph_content(&first));
    dir.write("app/src/a.ts", "export const a = 5;\n");
    let third = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&third);
    assert_ne!(third.value["id"], second.value["id"], "{}", third.value);
    let forced = ingest(
        &IngestOptions {
            path: dir.0.as_path().join("app"),
            workspace: Some(dir.0.as_path().to_path_buf()),
            refresh: true,
            ..Default::default()
        },
        &workspace_policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_fresh_build(&forced);
    assert_ne!(forced.value["id"], third.value["id"], "{}", forced.value);
    assert_eq!(graph_content(&forced), graph_content(&third));
}
