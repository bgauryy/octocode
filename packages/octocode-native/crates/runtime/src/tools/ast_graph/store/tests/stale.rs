//! Staleness: config inputs, added sources and cut snapshots.
use super::*;

fn stale_rows(dir: &Path) -> (Value, Value) {
    let out = ask(dir, "stale", None, |_| {});
    (out.value["fresh"].clone(), out.value["results"].clone())
}

/// A linking config (tsconfig, package.json, ...) beside the sources or up
/// the repository is a build input: editing, adding or removing one makes
/// the snapshot stale and stops ingest from reusing it, like a source edit.
#[test]
fn config_inputs_make_a_snapshot_stale_and_block_reuse() {
    let dir = fixture();
    dir.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
    dir.write("package.json", "{\"name\":\"repo\"}");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    assert_eq!(stale_rows(dir.0.as_path()).0, json!(true));
    assert_eq!(ingest_fixture(dir.0.as_path(), None).value["reused"], true);

    dir.write(
        "app/tsconfig.json",
        "{\"compilerOptions\":{\"paths\":{\"@x/*\":[\"src/*\"]}}}",
    );
    assert_eq!(
        stale_rows(dir.0.as_path()),
        (
            json!(false),
            json!([{"file": "tsconfig.json", "status": "changed", "config": true}])
        )
    );
    let rebuilt = ingest_fixture(dir.0.as_path(), None);
    assert_ne!(rebuilt.value["reused"], true, "{}", rebuilt.value);
    assert_eq!(stale_rows(dir.0.as_path()).0, json!(true));

    dir.write(
        "app/package.json",
        "{\"name\":\"app\",\"main\":\"src/main.ts\"}",
    );
    std::fs::remove_file(dir.0.as_path().join("package.json")).expect("remove");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([
            {"file": "../package.json", "status": "missing", "config": true},
            {"file": "package.json", "status": "added", "config": true},
        ])
    );
    assert_ne!(ingest_fixture(dir.0.as_path(), None).value["reused"], true);
}

/// `stale` rescans the scope like ingest: a source added since ingest is
/// reported, so the snapshot is never called fresh while a re-ingest would
/// rebuild it.
#[test]
fn stale_reports_sources_added_since_ingest() {
    let dir = fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    dir.write("app/src/z.ts", "export const z = 1;\n");
    dir.write("app/src/deep/new/y.ts", "export const y = 1;\n");
    let out = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(out.value["fresh"], false, "{}", out.value);
    assert_eq!(
        out.value["results"],
        json!([
            {"file": "src/deep/new/y.ts", "status": "added"},
            {"file": "src/z.ts", "status": "added"},
        ])
    );
    assert!(out.value["reingest"].is_string(), "{}", out.value);
    assert_ne!(ingest_fixture(dir.0.as_path(), None).value["reused"], true);
}

/// A snapshot whose scan stopped at maxFiles cannot tell an added file
/// from one past the cap, so it is never called fresh; its re-ingest keeps
/// the snapshot's exclusions and raises maxFiles, the limit that cut it.
#[test]
fn a_truncated_snapshot_is_never_fresh() {
    let dir = fixture();
    let out = ingest(
        &IngestOptions {
            path: dir.0.as_path().join("app"),
            workspace: Some(dir.0.as_path().to_path_buf()),
            max_files: Some(2),
            exclude_dir: vec!["gen".into()],
            ..Default::default()
        },
        &workspace_policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(out.value["scan"]["truncated"], true, "{}", out.value);
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], false, "{}", stale.value);
    assert!(
        stale.value["unchecked"]
            .as_str()
            .is_some_and(|why| why.contains("maxFiles (2)")),
        "{}",
        stale.value
    );
    // The cut's re-ingest is stated once, in the graph block.
    assert!(stale.value.get("reingest").is_none(), "{}", stale.value);
    let reingest = stale.value["graph"]["reingest"].as_str().expect("reingest");
    assert!(
        reingest.ends_with(" --max-files 4 --exclude gen"),
        "{reingest}"
    );
}

/// A snapshot published before snapshot bounds became build refusals may
/// carry `scan.edgesCapped`; it still parses and answers, and names no cut
/// the scan did not make.
#[test]
fn a_manifest_from_before_the_edge_bound_still_answers() {
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    assert!(
        out.value["scan"].get("edgesCapped").is_none(),
        "{}",
        out.value
    );
    let graph_dir = PathBuf::from(out.value["dir"].as_str().expect("dir"));
    let mut manifest = read_manifest(&graph_dir).expect("manifest");
    manifest["scan"]["edgesCapped"] = json!(true);
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).expect("manifest json"),
    )
    .expect("write manifest");
    let fresh = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(fresh.value["fresh"], true, "{}", fresh.value);
    assert!(
        fresh.value["graph"].get("partialReasons").is_none(),
        "{}",
        fresh.value
    );
    let stats = ask(dir.0.as_path(), "stats", None, |_| {});
    assert_eq!(stats.class, ExitClass::Success, "{}", stats.value);
}
