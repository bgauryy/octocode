//! A rebuild never invalidates a snapshot ID a reader still uses, and a
//! pruned ID names the snapshot that replaced it.
use super::*;

fn rebuild(dir: &Fixture, edit: u32) -> String {
    dir.write("app/a.ts", format!("export const a = {edit};\n"));
    let built = call(
        &dir.0,
        ToolId::GraphBuild,
        json!({"path":"app","keep":1}),
    );
    built["id"].as_str().expect("graph id").to_owned()
}

fn stats(dir: &Fixture, graph: &str) -> Value {
    call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"stats","path":"app","graph":graph}),
    )
}

fn home(dir: &Fixture) -> PathBuf {
    dir.0.join(".octocode").join("graph")
}

/// Backdates the last read of `id` past the lease.
fn expire_lease(dir: &Fixture, id: &str) {
    let marker = home(dir).join(id).join(READ_LEASE_FILE);
    let expired = std::time::SystemTime::now() - READ_LEASE - std::time::Duration::from_secs(1);
    std::fs::OpenOptions::new()
        .write(true)
        .open(marker)
        .expect("lease marker")
        .set_modified(expired)
        .expect("backdate lease");
}

#[test]
fn a_rebuild_keeps_a_snapshot_a_reader_used_within_its_lease() {
    let dir = Fixture::new();
    let first = rebuild(&dir, 1);
    let read = stats(&dir, &first);
    assert!(read.get("error").is_none(), "{read}");
    let second = rebuild(&dir, 2);
    assert_ne!(first, second);
    // keep:1 would have pruned `first`; its reader still walks it.
    let walked = stats(&dir, &first);
    assert!(walked.get("error").is_none(), "{walked}");
    assert_eq!(walked["id"], json!(first));
    assert_eq!(list_snapshots(&home(&dir)), [first.clone(), second.clone()]);

    // Once its reader is idle past the lease, the next rebuild prunes it.
    expire_lease(&dir, &first);
    let third = rebuild(&dir, 3);
    assert_eq!(list_snapshots(&home(&dir)), [third]);
}

#[test]
fn a_pruned_snapshot_id_names_its_successor() {
    let dir = Fixture::new();
    let first = rebuild(&dir, 1);
    let second = rebuild(&dir, 2);
    let stale = stats(&dir, &first);
    assert_eq!(stale["errorCode"], "notFound", "{stale}");
    assert_eq!(stale["supersededBy"], json!(second), "{stale}");
    let related = &stale["related"][0];
    assert_eq!(related["tool"], "graphQuery", "{stale}");
    let lead = related["input"]["queries"][0].clone();
    assert_eq!(lead["graph"], json!(second));
    let recovered = call(&dir.0, ToolId::GraphQuery, lead);
    assert_eq!(recovered["id"], json!(second), "{recovered}");
}

#[test]
fn an_unknown_id_is_not_called_superseded() {
    let dir = Fixture::new();
    rebuild(&dir, 1);
    for id in ["missing-snapshot", "20200101T000000Z-other-abcdef"] {
        let missing = stats(&dir, id);
        assert_eq!(missing["errorCode"], "notFound", "{missing}");
        assert!(missing.get("supersededBy").is_none(), "{missing}");
        assert_eq!(missing["related"][0]["tool"], "graphBuild", "{missing}");
    }
}
