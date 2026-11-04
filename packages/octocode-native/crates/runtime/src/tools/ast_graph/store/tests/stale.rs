//! Staleness: config inputs, added sources and cut snapshots.
use super::*;

fn stale_rows(dir: &Path) -> (Value, Value) {
    let out = ask(dir, "stale", None, |_| {});
    (out.value["fresh"].clone(), out.value["rows"].clone())
}

/// A linking config (tsconfig, package.json, ...) beside the sources or up
/// the repository is a build input: editing, adding or removing one makes
/// the snapshot stale, like a source edit. A new build captures the changed input.
#[test]
fn config_inputs_make_a_snapshot_stale_and_rebuild_fresh() {
    let dir = fixture();
    dir.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
    dir.write("package.json", "{\"name\":\"repo\"}");
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    assert_eq!(stale_rows(dir.0.as_path()).0, json!(true));
    assert_fresh_build(&ingest_fixture(dir.0.as_path(), None));

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
    assert_fresh_build(&rebuilt);
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
    assert_fresh_build(&ingest_fixture(dir.0.as_path(), None));
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
        out.value["rows"],
        json!([
            {"file": "src/deep/new/y.ts", "status": "added"},
            {"file": "src/z.ts", "status": "added"},
        ])
    );
    let reingest = &out.value["reingest"];
    assert_eq!(
        (&reingest["kind"], &reingest["tool"]),
        (&json!("related"), &json!("graphBuild")),
        "{}",
        out.value
    );
    assert!(
        reingest["input"]["queries"][0]["path"].is_string(),
        "{reingest}"
    );
    assert_fresh_build(&ingest_fixture(dir.0.as_path(), None));
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

/// Rewrites `path` with same-length `text` and puts its modification time
/// back: only the status change time still moves.
fn rewrite_keeping_mtime(path: &Path, text: &str) {
    let before = std::fs::metadata(path).expect("metadata");
    assert_eq!(before.len(), text.len() as u64, "same size");
    std::fs::write(path, text).expect("rewrite");
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(before.modified().expect("mtime"))
        .expect("restore mtime");
    let after = std::fs::metadata(path).expect("metadata");
    assert_eq!(after.modified().ok(), before.modified().ok());
}

/// A fixture with configs beside the sources, up the repository and ignored,
/// built after every file settled: its config inputs carry a stamp proof.
fn proven_fixture() -> Fixture {
    let dir = fixture();
    dir.write("package.json", "{\"name\":\"repo\"}");
    dir.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
    dir.write(
        "app/.gitignore",
        "compile_commands.json\nrequirements.txt\n",
    );
    dir.write("app/requirements.txt", "ignored==1\n");
    // A first build's graph home is a new entry of the workspace directory.
    std::fs::create_dir_all(dir.0.as_path().join(".octocode/graph")).expect("graph home");
    octocode_engine::stamp::settle_written_so_far();
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&built);
    let graph_dir = PathBuf::from(built.value["dir"].as_str().expect("dir"));
    let manifest = read_manifest(&graph_dir).expect("manifest");
    assert!(manifest["inputProof"].is_string(), "{manifest}");
    assert_eq!(stale_rows(dir.0.as_path()), (json!(true), json!([])));
    dir
}

/// An equal proof stands in for listing and hashing, like a source's equal
/// stamp: a recorded digest that could not match goes unread.
#[test]
fn an_equal_input_proof_reads_no_config() {
    let dir = proven_fixture();
    let latest =
        std::fs::read_to_string(dir.0.as_path().join(".octocode/graph/latest")).expect("latest");
    let graph_dir = dir.0.as_path().join(".octocode/graph").join(latest.trim());
    let mut manifest = read_manifest(&graph_dir).expect("manifest");
    manifest["inputs"]["tsconfig.json"] = json!("not-the-digest");
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest).expect("manifest json"),
    )
    .expect("write manifest");
    assert_eq!(stale_rows(dir.0.as_path()), (json!(true), json!([])));

    // Once a stamp moves, the digest decides again.
    rewrite_keeping_mtime(
        &dir.0.as_path().join("app/tsconfig.json"),
        "{\"compilerOptions\":{}}",
    );
    assert_eq!(
        stale_rows(dir.0.as_path()),
        (
            json!(false),
            json!([{"file": "tsconfig.json", "status": "changed", "config": true}])
        )
    );
}

/// The proof is as strict as a source's stamp: a config edit that keeps its
/// size and modification time still makes the snapshot stale.
#[test]
fn a_same_size_config_edit_with_mtime_restored_is_stale() {
    let dir = proven_fixture();
    rewrite_keeping_mtime(
        &dir.0.as_path().join("app/tsconfig.json"),
        "{\"compilerOptions\":{ }",
    );
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "tsconfig.json", "status": "changed", "config": true}])
    );
}

/// The same for a config above the scope and for one `.gitignore` hides from
/// the source walk, which only the config scan sees.
#[test]
fn an_ancestor_or_ignored_config_edit_is_stale() {
    let dir = proven_fixture();
    rewrite_keeping_mtime(&dir.0.as_path().join("package.json"), "{\"name\":\"rep0\"}");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "../package.json", "status": "changed", "config": true}])
    );
    let dir = proven_fixture();
    rewrite_keeping_mtime(
        &dir.0.as_path().join("app/requirements.txt"),
        "ignored==2\n",
    );
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "requirements.txt", "status": "changed", "config": true}])
    );
}

#[test]
fn a_same_size_source_edit_with_mtime_restored_is_stale() {
    let dir = proven_fixture();
    let source = dir.0.as_path().join("app/src/b.ts");
    let text = std::fs::read_to_string(&source).expect("source");
    rewrite_keeping_mtime(&source, &text.replace("a()", "c()"));
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "src/b.ts", "status": "changed"}])
    );
}

#[test]
fn a_source_added_under_a_proven_snapshot_is_stale() {
    let dir = proven_fixture();
    dir.write("app/src/z.ts", "export const z = 1;\n");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "src/z.ts", "status": "added"}])
    );
}

/// A config added in a scanned directory, seen by the source walk or hidden
/// from it by `.gitignore`, and one removed: each is a changed build input.
#[test]
fn configs_added_or_removed_under_a_proven_snapshot_are_stale() {
    let dir = proven_fixture();
    dir.write("app/src/go.mod", "module x\n");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "src/go.mod", "status": "added", "config": true}])
    );
    let dir = proven_fixture();
    dir.write("app/compile_commands.json", "[]\n");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "compile_commands.json", "status": "added", "config": true}])
    );
    let dir = proven_fixture();
    std::fs::remove_file(dir.0.as_path().join("package.json")).expect("remove");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "../package.json", "status": "missing", "config": true}])
    );
}

/// The repository root bounds which ancestors are inputs: moving it changes
/// the scope itself, so the proof cannot hold.
#[test]
fn a_moved_repository_root_is_stale_under_a_proof() {
    let dir = proven_fixture();
    std::fs::remove_dir_all(dir.0.as_path().join(".git")).expect("remove marker");
    assert_eq!(
        stale_rows(dir.0.as_path()).1,
        json!([{"file": "../package.json", "status": "missing", "config": true}])
    );
}

/// A file written inside the settle window leaves no proof: its stamp could
/// not show a later write in the same tick. The digests still decide.
#[test]
fn an_unsettled_input_records_no_proof() {
    let dir = fixture();
    dir.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&built);
    let graph_dir = PathBuf::from(built.value["dir"].as_str().expect("dir"));
    let manifest = read_manifest(&graph_dir).expect("manifest");
    assert!(manifest.get("inputProof").is_none(), "{manifest}");
    assert_eq!(stale_rows(dir.0.as_path()), (json!(true), json!([])));
}

/// A config the path policy withheld from the build is not an input today,
/// and is one when the policy admits it later. No proof can vouch for that,
/// since the config has no recorded stamp, so none is recorded.
#[test]
fn a_config_the_policy_withheld_records_no_proof() {
    let dir = fixture();
    dir.write("package.json", "{\"name\":\"repo\"}");
    dir.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
    let root = dir.0.as_path().join("app");
    std::fs::create_dir_all(root.join(".octocode/graph")).expect("graph home");
    octocode_engine::stamp::settle_written_so_far();
    let options = IngestOptions {
        path: root.clone(),
        workspace: Some(root.clone()),
        ..IngestOptions::default()
    };
    let built = ingest(
        &options,
        &workspace_policy(&root),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let graph_dir = PathBuf::from(built.value["dir"].as_str().expect("dir"));
    let manifest = read_manifest(&graph_dir).expect("manifest");
    assert_eq!(
        manifest["inputs"].as_object().map(serde_json::Map::len),
        Some(1)
    );
    assert!(manifest.get("inputProof").is_none(), "{manifest}");
}
