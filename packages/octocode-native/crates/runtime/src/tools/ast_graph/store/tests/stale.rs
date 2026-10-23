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
        &policy(dir.0.as_path()),
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

/// The edge cap does not cut the file scan: a capped snapshot whose files
/// were all scanned still reports files added since ingest, names no
/// maxFiles cut, and is fresh once nothing changed.
#[test]
fn an_edge_capped_snapshot_still_checks_added_files() {
    struct EdgeCap;
    impl Drop for EdgeCap {
        fn drop(&mut self) {
            crate::tools::ast_graph::graph::EDGE_CAP
                .with(|edges| edges.set(crate::tools::ast_graph::graph::MAX_GRAPH_EDGES));
        }
    }
    crate::tools::ast_graph::graph::EDGE_CAP.with(|edges| edges.set(1));
    let _restore = EdgeCap;
    let dir = fixture();
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.value["scan"]["edgesCapped"], true, "{}", out.value);
    assert_eq!(out.value["scan"]["truncated"], false, "{}", out.value);
    let fresh = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(fresh.value["fresh"], true, "{}", fresh.value);
    assert!(fresh.value.get("reingest").is_none(), "{}", fresh.value);
    dir.write("app/src/z.ts", "export const z = 1;\n");
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(
        stale.value["results"],
        json!([{"file": "src/z.ts", "status": "added"}]),
        "{}",
        stale.value
    );
    assert!(stale.value.get("unchecked").is_none(), "{}", stale.value);
    let reingest = stale.value["reingest"].as_str().expect("reingest");
    assert!(!reingest.contains("--max-files"), "{reingest}");
}
