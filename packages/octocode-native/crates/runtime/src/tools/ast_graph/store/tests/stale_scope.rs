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
