//! A snapshot's listing proof: an equal proof stands in for the source walk,
//! and whatever a walk would list differently moves it.
use super::*;

fn stale_rows(dir: &Path) -> Value {
    ask(dir, "stale", None, |_| {}).value["rows"].clone()
}

/// A fixture with a directory holding no source and an ignored source, built
/// after everything settled, so its manifest holds a listing proof.
fn proven() -> Fixture {
    let dir = fixture();
    dir.write("app/docs/note.md", "notes\n");
    dir.write("app/.gitignore", "gen/\n");
    dir.write("app/gen/g.ts", "export const g = 1;\n");
    std::fs::create_dir_all(dir.0.as_path().join(".octocode/graph")).expect("graph home");
    octocode_engine::stamp::settle_written_so_far();
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&built);
    let graph_dir = PathBuf::from(built.value["dir"].as_str().expect("dir"));
    let manifest = read_manifest(&graph_dir).expect("manifest");
    assert!(manifest["scopeProof"]["stamps"].is_string(), "{manifest}");
    assert_eq!(stale_rows(dir.0.as_path()), json!([]));
    dir
}

#[test]
fn an_equal_listing_proof_walks_nothing() {
    let dir = proven();
    let before = SCOPE_WALKS.with(std::cell::Cell::get);
    assert_eq!(stale_rows(dir.0.as_path()), json!([]));
    assert_eq!(SCOPE_WALKS.with(std::cell::Cell::get), before);
}

#[test]
fn a_source_added_where_the_snapshot_holds_nothing_is_stale() {
    let dir = proven();
    dir.write("app/docs/n.ts", "export const n = 1;\n");
    assert_eq!(
        stale_rows(dir.0.as_path()),
        json!([{"file": "docs/n.ts", "status": "added"}])
    );
    let dir = proven();
    dir.write("app/new/deep/n.ts", "export const n = 1;\n");
    assert_eq!(
        stale_rows(dir.0.as_path()),
        json!([{"file": "new/deep/n.ts", "status": "added"}])
    );
}

/// An ignore file edited in place, its size and modification time kept, says
/// what a listing now holds.
#[test]
fn an_ignore_file_edit_that_keeps_its_times_is_stale() {
    let dir = proven();
    let path = dir.0.as_path().join("app/.gitignore");
    let modified = std::fs::metadata(&path)
        .expect("metadata")
        .modified()
        .expect("mtime");
    std::fs::write(&path, "gem/\n").expect("rewrite");
    std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(modified)
        .expect("restore mtime");
    assert_eq!(
        stale_rows(dir.0.as_path()),
        json!([{"file": "gen/g.ts", "status": "added"}])
    );
}

#[test]
fn an_ignore_input_added_anywhere_is_stale() {
    let excluded = json!([{"file": "src/a.ts", "status": "excluded"}]);
    let dir = proven();
    dir.write("app/src/.gitignore", "a.ts\n");
    assert_eq!(stale_rows(dir.0.as_path()), excluded);
    let dir = proven();
    dir.write(".gitignore", "a.ts\n");
    assert_eq!(stale_rows(dir.0.as_path()), excluded);
    let dir = proven();
    dir.write(".git/info/exclude", "a.ts\n");
    assert_eq!(stale_rows(dir.0.as_path()), excluded);
}

/// Git's rules apply only inside a repository: its marker going lifts them.
#[test]
fn a_repository_marker_gone_is_stale() {
    let dir = proven();
    std::fs::remove_dir_all(dir.0.as_path().join(".git")).expect("marker");
    assert_eq!(
        stale_rows(dir.0.as_path()),
        json!([{"file": "gen/g.ts", "status": "added"}])
    );
}

#[test]
fn an_unsettled_listing_records_no_proof() {
    let dir = fixture();
    dir.date_ahead("app/src");
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_fresh_build(&built);
    let graph_dir = PathBuf::from(built.value["dir"].as_str().expect("dir"));
    let manifest = read_manifest(&graph_dir).expect("manifest");
    assert!(manifest.get("scopeProof").is_none(), "{manifest}");
    assert_eq!(stale_rows(dir.0.as_path()), json!([]));
    dir.write("app/src/z.ts", "export const z = 1;\n");
    assert_eq!(
        stale_rows(dir.0.as_path()),
        json!([{"file": "src/z.ts", "status": "added"}])
    );
}

/// The stale check walks what the build walks: a source the path policy
/// withholds under a lifted walk (`.env.py`, a credential store's file) is
/// no listing difference, so the snapshot is fresh and its listing proven.
#[test]
fn a_withheld_source_under_a_lifted_walk_is_fresh() {
    use crate::tools::ast_graph::walk_scope::WalkScope;
    use octocode_engine::types::IgnorePolicy;
    let dir = fixture();
    dir.write("app/.hid/h.py", "def h():\n    pass\n");
    dir.write("app/.env.py", "def env():\n    pass\n");
    dir.write("app/.ssh/k.py", "def k():\n    pass\n");
    std::fs::create_dir_all(dir.0.as_path().join(".octocode/graph")).expect("graph home");
    octocode_engine::stamp::settle_written_so_far();
    let built = ingest(
        &IngestOptions {
            path: dir.0.join("app"),
            workspace: Some(dir.0.to_path_buf()),
            walk: WalkScope {
                ignore: IgnorePolicy {
                    include_ignored: false,
                    include_hidden: true,
                },
                ..WalkScope::default()
            },
            ..Default::default()
        },
        &workspace_policy(dir.0.as_path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
    );
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let graph_dir = PathBuf::from(built.value["dir"].as_str().expect("dir"));
    let manifest = read_manifest(&graph_dir).expect("manifest");
    assert!(manifest["scopeProof"]["stamps"].is_string(), "{manifest}");
    let stale = ask(dir.0.as_path(), "stale", None, |_| {});
    assert_eq!(stale.value["fresh"], true, "{}", stale.value);
    assert_eq!(stale.value["rows"], json!([]), "{}", stale.value);
    // The proof stands in for the walk; without it the walk must agree too.
    let mut bare = manifest;
    bare.as_object_mut().expect("manifest").remove("scopeProof");
    std::fs::write(
        graph_dir.join(MANIFEST_FILE),
        serde_json::to_vec(&bare).expect("manifest json"),
    )
    .expect("write manifest");
    assert_eq!(stale_rows(dir.0.as_path()), json!([]));
}
