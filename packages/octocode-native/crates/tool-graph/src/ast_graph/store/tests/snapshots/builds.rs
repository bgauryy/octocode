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

/// A build of unchanged inputs answers with the current snapshot itself.
#[test]
fn a_build_of_unchanged_inputs_answers_with_the_current_snapshot() {
    let dir = fixture();
    let first = ingest_fixture(&dir.0, None);
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    assert!(first.value.get("reused").is_none(), "{}", first.value);
    let second = ingest_fixture(&dir.0, None);
    assert_fresh_build(&second);
    assert_eq!(
        first.value["graph"], second.value["graph"],
        "{}",
        second.value
    );
    assert_eq!(second.value["reused"], true, "{}", second.value);
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
    assert_eq!(
        second.value["graph"], first.value["graph"],
        "{}",
        second.value
    );
    assert_eq!(graph_content(&second), graph_content(&first));
    dir.write("app/src/a.ts", "export const a = 5;\n");
    let third = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&third);
    assert_ne!(
        third.value["graph"], second.value["graph"],
        "{}",
        third.value
    );
    assert!(third.value.get("reused").is_none(), "{}", third.value);
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
    assert_ne!(
        forced.value["graph"], third.value["graph"],
        "{}",
        forced.value
    );
    assert!(forced.value.get("reused").is_none(), "{}", forced.value);
    assert_eq!(graph_content(&forced), graph_content(&third));
}

/// A snapshot is reused only when the build identity agrees. Other options
/// that select the same files change the identity alone: the tree and every
/// representation version in the manifest stay intact, yet the next build
/// must not answer with the snapshot built under the old identity.
#[test]
fn only_the_build_identity_decides_between_reuse_and_rebuild() {
    let dir = fixture();
    // A snapshot records a source's stamp only once it settled: settle the
    // fixture on this thread's clock, so both builds record the same stamps
    // however long the first one took.
    octocode_engine::stamp::settle_written_so_far();
    let home = dir.0.as_path().join(".octocode").join("graph");
    let build = |exclude: &[&str]| {
        ingest(
            &IngestOptions {
                path: dir.0.as_path().join("app"),
                workspace: Some(dir.0.as_path().to_path_buf()),
                exclude: exclude.iter().map(|glob| (*glob).to_owned()).collect(),
                ..Default::default()
            },
            &workspace_policy(dir.0.as_path()),
            &ContentSecurity::new(),
            &NeverCancel,
            None,
        )
    };
    let manifest_of = |out: &GraphOutput| {
        let id = out.value["graph"].as_str().expect("id");
        read_manifest(&home.join(id)).expect("manifest")
    };
    let first = build(&[]);
    assert_fresh_build(&first);
    // A glob that selects no file: same tree, same scan, other identity.
    let other = build(&["no-such-dir/**"]);
    assert_fresh_build(&other);
    assert_ne!(id_of(&other), id_of(&first), "{}", other.value);
    let (before, after) = (manifest_of(&first), manifest_of(&other));
    assert_ne!(before["buildIdentity"], after["buildIdentity"]);
    for version in ["formatVersion", "analysisVersion", "factsSchemaVersion"] {
        assert_eq!(before[version], after[version], "{version}");
    }
    assert_eq!(before["graph"]["sha256"], after["graph"]["sha256"]);
    // Control: give the newest snapshot the identity of the default build and
    // the very same request reuses it, so identity is what decided above.
    let manifest_path = home.join(id_of(&other)).join(MANIFEST_FILE);
    let mut forged = after.clone();
    forged["buildIdentity"] = before["buildIdentity"].clone();
    std::fs::write(&manifest_path, forged.to_string()).expect("forged manifest");
    let reused = build(&[]);
    assert_eq!(reused.class, ExitClass::Success, "{}", reused.value);
    assert_eq!(id_of(&reused), id_of(&other), "{}", reused.value);
}

fn id_of(out: &GraphOutput) -> &str {
    out.value["graph"].as_str().expect("id")
}
