use anyhow::Result;
use octocode_agents_communication::{
    database::{execute, query},
    store::{Store, now},
};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::mpsc, thread, time::Duration};
use tempfile::{TempDir, tempdir};

struct Fixture {
    _dir: TempDir,
    store: Store,
    a: String,
    b: String,
}
impl Fixture {
    fn new() -> Result<Self> {
        let dir = tempdir()?;
        let store = Store::open(dir.path().join("v1.sqlite"), dir.path(), false, true)?;
        let a = store.call("", "join", &json!({"name":"a","vendor":"codex"}))?["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("id"))?
            .into();
        let b = store.call("", "join", &json!({"name":"b","vendor":"claude"}))?["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("id"))?
            .into();
        Ok(Self {
            _dir: dir,
            store,
            a,
            b,
        })
    }
    fn sql(&self, sql: &str, args: &[Value]) -> Result<usize> {
        execute(&Connection::open(&self.store.database)?, sql, args)
    }
}
#[test]
fn idle_claim_does_not_wait_for_writer() -> Result<()> {
    let f = Fixture::new()?;
    let blocker = Connection::open(&f.store.database)?;
    blocker.execute_batch("BEGIN IMMEDIATE")?;
    let started = std::time::Instant::now();
    assert!(f.store.claim(&f.a, "idle-worker")?.is_empty());
    assert!(started.elapsed() < Duration::from_secs(1));
    blocker.execute_batch("ROLLBACK")?;
    Ok(())
}
#[test]
fn lock_overlap_generation_and_expiry() -> Result<()> {
    let f = Fixture::new()?;
    let first = f
        .store
        .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"src","kind":"tree"}))?;
    assert_eq!(first["ok"], true);
    assert_eq!(
        f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"src/file"}))?["ok"],
        false
    );
    assert_eq!(
        f.store
            .call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"src-other/file"}))?["ok"],
        true
    );
    f.sql(
        "UPDATE leases SET expiresAt=0 WHERE id=?",
        &[first["lease"]["id"].clone()],
    )?;
    let next = f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"src/file"}))?;
    assert_eq!(next["ok"], true);
    assert_eq!(
        f.store
            .call(&f.a, "renew", &json!({"lease":first["lease"]["id"]}))?["renewed"],
        false
    );
    assert_eq!(
        f.store
            .call(&f.a, "unlock", &json!({"lease":next["lease"]["id"]}))?["released"],
        false
    );
    Ok(())
}
#[test]
fn lease_ttl_starts_after_writer_wait() -> Result<()> {
    let f = Fixture::new()?;
    let database = f.store.database.clone();
    let workspace = PathBuf::from(&f.store.workspace);
    let actor = f.a.clone();
    let (ready, wait) = mpsc::channel();
    let (go, start) = mpsc::channel();
    let worker = thread::spawn(move || -> Result<Value> {
        let store = Store::open(database, &workspace, false, false)?;
        ready.send(())?;
        start.recv()?;
        store.call(&actor, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"blocked","ttlMs":1000}))
    });
    wait.recv()?;
    let blocker = Connection::open(&f.store.database)?;
    blocker.execute_batch("BEGIN IMMEDIATE")?;
    go.send(())?;
    thread::sleep(Duration::from_millis(1200));
    blocker.execute_batch("COMMIT")?;
    let result = worker
        .join()
        .map_err(|_| anyhow::anyhow!("worker panicked"))??;
    assert_eq!(result["ok"], true);
    assert!(result["lease"]["expiresAt"].as_i64().unwrap_or(0) - now() > 700);
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlinks_and_missing_paths_are_canonical() -> Result<()> {
    use std::os::unix::fs::symlink;
    let f = Fixture::new()?;
    let root = PathBuf::from(&f.store.workspace);
    fs::create_dir(root.join("real"))?;
    symlink(root.join("real"), root.join("alias"))?;
    symlink(root.join("real/missing"), root.join("dangling"))?;
    f.store
        .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"real/missing"}))?;
    for path in ["alias/missing", "dangling", "other/../real/missing"] {
        assert_eq!(
            f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":path}))?["ok"],
            false
        );
    }
    assert!(
        f.store
            .call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"../escape"}))
            .is_err()
    );
    Ok(())
}
#[test]
fn portable_case_and_unicode_aliases_conflict_before_creation() -> Result<()> {
    assert_eq!(caseless::UNICODE_VERSION, (16, 0, 0));
    assert_eq!(unicode_normalization::UNICODE_VERSION, (16, 0, 0));
    let f = Fixture::new()?;
    for (first, second) in [
        ("NEW.txt", "new.txt"),
        ("CAFÉ/file", "cafe\u{301}/FILE"),
        ("Straße", "STRASSE"),
    ] {
        let held = f.store.call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":first}))?;
        assert_eq!(held["ok"], true);
        assert_eq!(
            f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":second}))?["ok"],
            false
        );
        let view = f
            .store
            .entity_list(&f.b, "lease", &json!({"path":second}))?;
        assert_eq!(view["items"][0]["id"], held["lease"]["id"]);
        f.store
            .call(&f.a, "unlock", &json!({"lease":held["lease"]["id"]}))?;
    }
    f.store
        .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"SRC","kind":"tree"}))?;
    assert_eq!(
        f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"src/nested"}))?["ok"],
        false
    );
    assert_eq!(
        f.store
            .call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"src-other/nested"}))?["ok"],
        true
    );
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlink_parent_traversal_matches_os_identity() -> Result<()> {
    use std::os::unix::fs::symlink;
    let f = Fixture::new()?;
    let root = PathBuf::from(&f.store.workspace);
    fs::create_dir_all(root.join("real/sub"))?;
    fs::write(root.join("real/shared.txt"), "target")?;
    symlink("real/sub", root.join("alias"))?;
    let held = f
        .store
        .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"alias/../shared.txt"}))?;
    assert_eq!(
        held["lease"]["path"],
        json!(fs::canonicalize(root.join("alias/../shared.txt"))?)
    );
    assert_eq!(
        f.store
            .call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"real/shared.txt"}))?["ok"],
        false
    );
    symlink("real/missing/../new", root.join("dangling-parent"))?;
    f.store
        .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"dangling-parent"}))?;
    assert_eq!(
        f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"real/new"}))?["ok"],
        false
    );
    symlink(".", root.join("self"))?;
    assert!(
        f.store
            .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"self/../outside"}))
            .is_err()
    );
    symlink("cycle", root.join("cycle"))?;
    assert!(
        f.store
            .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"cycle"}))
            .is_err()
    );
    Ok(())
}
#[test]
fn durable_messages_claim_recovery_and_ack() -> Result<()> {
    let f = Fixture::new()?;
    let args = json!({"reasoning":"Verify durable message receipt and recovery","to":f.b,"body":"hello","key":"one"});
    let sent = f.store.call(&f.a, "send_message", &args)?;
    assert_eq!(f.store.call(&f.a, "send_message", &args)?["id"], sent["id"]);
    assert!(
        f.store
            .call(
                &f.a,
                "send_message",
                &json!({"reasoning":"Exercise send_message contract in an isolated regression fixture","to":f.b,"body":"changed","key":"one"})
            )
            .is_err()
    );
    assert_eq!(f.store.claim(&f.b, "old")?.len(), 1);
    assert!(f.store.claim(&f.b, "other")?.is_empty());
    f.sql("UPDATE deliveries SET claimUntil=0", &[])?;
    assert!(
        f.store.claim(&f.b, "old")?.is_empty(),
        "same live worker must not repeat history"
    );
    assert_eq!(f.store.claim(&f.b, "new")?.len(), 1);
    assert_eq!(
        f.store.call(&f.a, "ack", &json!({"message":sent["id"]}))?["acknowledged"],
        false
    );
    assert_eq!(
        f.store.call(&f.b, "ack", &json!({"message":sent["id"]}))?["acknowledged"],
        true
    );
    assert_eq!(
        f.store.call(&f.b, "ack", &json!({"message":sent["id"]}))?["acknowledged"],
        true
    );
    assert_eq!(f.store.inbox(&f.b, 0)?["items"], json!([]));
    Ok(())
}
#[test]
fn topics_snapshot_and_resume_do_not_revive_leases() -> Result<()> {
    let f = Fixture::new()?;
    f.store
        .call(&f.b, "subscribe", &json!({"topics":["build"]}))?;
    f.store.call(
        &f.a,
        "send_message",
        &json!({"reasoning":"Exercise send_message contract in an isolated regression fixture","topic":"build","body":"ready"}),
    )?;
    let lease = f
        .store
        .call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"owned","ttlMs":600000}))?;
    assert_eq!(f.store.inbox(&f.a, 0)?["items"], json!([]));
    assert_eq!(f.store.claim(&f.b, "old")?.len(), 1);
    assert!(
        f.store
            .call(&f.b, "resume", &json!({"vendor":"claude"}))
            .is_err()
    );
    f.sql("UPDATE sessions SET expiresAt=0 WHERE id=?", &[json!(f.b)])?;
    assert!(f.store.call(&f.b, "heartbeat", &json!({})).is_err());
    f.store
        .call(&f.a, "send_message", &json!({"reasoning":"Exercise send_message contract in an isolated regression fixture","to":f.b,"body":"offline"}))?;
    f.store.call(&f.b, "resume", &json!({"vendor":"claude"}))?;
    assert_eq!(f.store.claim(&f.b, "new")?.len(), 2);
    assert_eq!(
        f.store
            .call(&f.b, "renew", &json!({"lease":lease["lease"]["id"]}))?["renewed"],
        false
    );
    f.sql("UPDATE messages SET expiresAt=0", &[])?;
    assert_eq!(f.store.inbox(&f.b, 0)?["items"], json!([]));
    Ok(())
}
#[test]
fn entity_visibility_updates_and_pagination() -> Result<()> {
    let f = Fixture::new()?;
    assert_eq!(
        f.store.entity_set(
            &f.a,
            "session",
            &f.a,
            &json!({"name":"renamed","vendorSession":null})
        )?["name"],
        "renamed"
    );
    assert!(
        f.store
            .entity_set(&f.a, "session", &f.b, &json!({"name":"wrong"}))
            .is_err()
    );
    assert!(
        f.store
            .entity_set(&f.a, "session", &f.a, &json!({"expiresAt":999999}))
            .is_err()
    );
    f.store
        .entity_set(&f.a, "subscriptions", &f.a, &json!({"topics":["a","a"]}))?;
    assert_eq!(
        f.store.entity_get(&f.a, "subscriptions", &f.a)?["topics"],
        json!(["a"])
    );
    for i in 0..105 {
        f.store.call(
            &f.a,
            "send_message",
            &json!({"reasoning":"Exercise send_message contract in an isolated regression fixture","to":f.b,"body":i.to_string()}),
        )?;
    }
    let first = f
        .store
        .entity_list(&f.a, "message", &json!({"direction":"sent"}))?;
    let last = f.store.entity_list(
        &f.a,
        "message",
        &json!({"direction":"sent","after":first["next"]}),
    )?;
    assert_eq!(first["items"].as_array().map(Vec::len), Some(100));
    assert_eq!(last["items"].as_array().map(Vec::len), Some(5));
    assert!(last["next"].is_null());
    let inbox = f.store.inbox(&f.b, 0)?;
    assert_eq!(inbox["items"].as_array().map(Vec::len), Some(100));
    let tail = f.store.inbox(&f.b, inbox["next"].as_i64().unwrap_or(0))?;
    assert_eq!(tail["items"].as_array().map(Vec::len), Some(5));
    assert!(tail["next"].is_null());
    let outsider = f
        .store
        .call("", "join", &json!({"name":"c","vendor":"other"}))?;
    let c = outsider["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("id"))?;
    assert!(f.store.entity_get(c, "message", "1")?.is_null());
    let directory = tempdir()?;
    let other = Store::open(f.store.database.clone(), directory.path(), false, false)?;
    assert!(other.inbox(&f.a, 0).is_err());
    let delivery = f
        .store
        .entity_get(&f.a, "delivery", &format!("1:{}", f.b))?;
    assert!(delivery["acknowledgedAt"].is_null());
    Ok(())
}
#[test]
fn incompatible_schema_is_never_repaired() -> Result<()> {
    let f = Fixture::new()?;
    f.sql("DROP TABLE subscriptions", &[])?;
    let bytes = fs::read(&f.store.database)?;
    assert!(
        Store::open(
            f.store.database.clone(),
            PathBuf::from(&f.store.workspace).as_path(),
            false,
            false
        )
        .is_err()
    );
    assert_eq!(bytes, fs::read(&f.store.database)?);
    assert!(
        query(
            &Connection::open(&f.store.database)?,
            "SELECT name FROM sqlite_schema WHERE name='subscriptions'",
            &[]
        )?
        .is_empty()
    );
    Ok(())
}

#[test]
fn notify_all_snapshots_active_workspace_peers_and_retries_once() -> Result<()> {
    let f = Fixture::new()?;
    let c = f
        .store
        .call("", "join", &json!({"name":"c","vendor":"pi"}))?;
    let c = c["id"].as_str().ok_or_else(|| anyhow::anyhow!("id"))?;
    let elsewhere = tempdir()?;
    let outside = Store::open(f.store.database.clone(), elsewhere.path(), false, false)?;
    let outsider = outside.call("", "join", &json!({"name":"outside","vendor":"other"}))?;
    let outsider = outsider["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("id"))?;
    f.store.call(c, "leave", &json!({}))?;
    let input = json!({"reasoning":"Verify one broadcast snapshots active peers","body":"all active peers","key":"broadcast-once"});
    let sent = f.store.call(&f.a, "notify_all", &input)?;
    assert_eq!(sent["recipients"], 1);
    assert_eq!(f.store.inbox(&f.b, 0)?["items"][0]["id"], sent["id"]);
    assert_eq!(f.store.inbox(&f.a, 0)?["items"], json!([]));
    assert_eq!(outside.inbox(outsider, 0)?["items"], json!([]));
    f.store.call(c, "resume", &json!({"vendor":"pi"}))?;
    assert_eq!(f.store.call(&f.a, "notify_all", &input)?, sent);
    assert_eq!(f.store.inbox(c, 0)?["items"], json!([]));
    assert!(
        f.store
            .call(
                &f.a,
                "notify_all",
                &json!({"reasoning":"Exercise notify_all contract in an isolated regression fixture","body":"changed","key":"broadcast-once"})
            )
            .is_err()
    );
    assert!(
        f.store
            .call(&f.a, "notify_all", &json!({"reasoning":"Exercise notify_all contract in an isolated regression fixture","body":"bad","to":f.b}))
            .is_err()
    );
    assert_eq!(
        f.store
            .call(&f.a, "notify_all", &json!({"reasoning":"Exercise notify_all contract in an isolated regression fixture","body":"new snapshot"}))?["recipients"],
        2
    );
    f.store.call(&f.b, "leave", &json!({}))?;
    f.store.call(c, "leave", &json!({}))?;
    assert_eq!(
        f.store.call(&f.a, "notify_all", &json!({"reasoning":"Exercise notify_all contract in an isolated regression fixture","body":"alone"}))?["recipients"],
        0
    );
    f.store.call(&f.a, "leave", &json!({}))?;
    assert!(
        f.store
            .call(&f.a, "notify_all", &json!({"reasoning":"Exercise notify_all contract in an isolated regression fixture","body":"expired"}))
            .is_err()
    );
    Ok(())
}

#[test]
fn notify_all_rolls_back_message_and_partial_fanout_on_failure() -> Result<()> {
    let f = Fixture::new()?;
    f.store
        .call("", "join", &json!({"name":"third","vendor":"pi"}))?;
    // Abort the second delivery, after one recipient insert has already succeeded.
    f.sql("CREATE TRIGGER fail_second_delivery BEFORE INSERT ON deliveries WHEN (SELECT count(*) FROM deliveries)>0 BEGIN SELECT RAISE(ABORT, 'injected delivery failure'); END", &[])?;
    assert!(
        f.store
            .call(&f.a, "notify_all", &json!({"reasoning":"Exercise notify_all contract in an isolated regression fixture","body":"atomic"}))
            .is_err()
    );
    let db = Connection::open(&f.store.database)?;
    assert_eq!(
        query(&db, "SELECT count(*) n FROM messages", &[])?[0]["n"],
        0
    );
    assert_eq!(
        query(&db, "SELECT count(*) n FROM deliveries", &[])?[0]["n"],
        0
    );
    Ok(())
}

#[test]
fn deleting_and_recreating_files_does_not_release_path_reservations() -> Result<()> {
    let f = Fixture::new()?;
    let root = PathBuf::from(&f.store.workspace);
    fs::create_dir(root.join("tree"))?;
    fs::write(root.join("tree/file"), "before")?;
    let held = f
        .store
        .call(&f.a, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"tree","kind":"tree"}))?;
    fs::remove_dir_all(root.join("tree"))?;
    assert_eq!(
        f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"tree/file"}))?["ok"],
        false
    );
    fs::create_dir(root.join("tree"))?;
    fs::write(root.join("tree/file"), "after")?;
    assert_eq!(
        f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"tree/file"}))?["ok"],
        false
    );
    assert_eq!(
        f.store
            .call(&f.a, "unlock", &json!({"lease":held["lease"]["id"]}))?["released"],
        true
    );
    let next = f.store.call(&f.b, "lock", &json!({"reasoning":"Exercise lock contract in an isolated regression fixture","path":"tree/file"}))?;
    assert_eq!(next["ok"], true);
    assert_ne!(next["lease"]["id"], held["lease"]["id"]);
    assert_eq!(
        f.store
            .call(&f.a, "unlock", &json!({"lease":held["lease"]["id"]}))?["released"],
        false
    );
    Ok(())
}

#[test]
fn pruning_preserves_audit_messages_and_deliveries() -> Result<()> {
    let f = Fixture::new()?;
    let old = f
        .store
        .call(&f.a, "send_message", &json!({"reasoning":"Exercise send_message contract in an isolated regression fixture","to":f.b,"body":"old"}))?;
    let live = f
        .store
        .call(&f.a, "send_message", &json!({"reasoning":"Exercise send_message contract in an isolated regression fixture","to":f.b,"body":"live"}))?;
    f.sql(
        "UPDATE messages SET expiresAt=0 WHERE id=?",
        &[old["id"].clone()],
    )?;
    f.store.call("", "prune", &json!({}))?;
    let db = Connection::open(&f.store.database)?;
    assert!(
        !query(
            &db,
            "SELECT * FROM deliveries WHERE message=?",
            &[old["id"].clone()]
        )?
        .is_empty()
    );
    assert_eq!(f.store.inbox(&f.b, 0)?["items"][0]["id"], live["id"]);
    assert_eq!(
        query(&db, "SELECT count(*) n FROM sessions", &[])?[0]["n"],
        2
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn live_store_rejects_deleted_or_replaced_database() -> Result<()> {
    let f = Fixture::new()?;
    let moved = f.store.database.with_extension("old");
    fs::rename(&f.store.database, &moved)?;
    assert!(
        f.store.claim(&f.a, "worker").is_err(),
        "deleted DB must not leave an idle proxy on an orphaned connection"
    );
    fs::write(&f.store.database, "replacement")?;
    assert!(
        f.store.call("", "peers", &json!({})).is_err(),
        "replacement must not preserve an orphaned store"
    );
    Ok(())
}

#[test]
fn passive_mail_waits_for_action_and_action_cannot_be_starved() -> Result<()> {
    let f = Fixture::new()?;
    for index in 0..19 {
        f.store.call(&f.a, "send_message", &json!({"to":f.b,"body":format!("FYI {index}"),"reasoning":"Record passive facts","wake":"passive"}))?;
    }
    assert!(f.store.stage(&f.b, "managed:codex")?.is_empty());
    let db = Connection::open(&f.store.database)?;
    assert_eq!(
        query(&db, "SELECT count(*) AS n FROM dispatches", &[])?[0]["n"],
        0
    );
    let action = f.store.call(
        &f.a,
        "send_message",
        &json!({"to":f.b,"body":"Handle the authorized request","reasoning":"Resume useful work"}),
    )?;
    let items = f.store.stage(&f.b, "managed:codex")?;
    assert_eq!(items.len(), 16);
    assert_eq!(items[0]["id"], action["id"]);
    f.store.finish_dispatch(&f.b, &items, None)?;
    assert!(f.store.stage(&f.b, "managed:codex")?.is_empty());
    assert_eq!(
        f.store.inbox(&f.b, 0)?["items"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Expected array"))?
            .len(),
        20
    );
    assert_eq!(f.store.stage(&f.b, "initial")?.len(), 4);
    Ok(())
}

#[test]
fn wake_intent_is_idempotent_and_broadcast_defaults_passive() -> Result<()> {
    let f = Fixture::new()?;
    f.store.call(
        &f.a,
        "notify_all",
        &json!({"body":"FYI","reasoning":"Broadcast fact"}),
    )?;
    assert!(!f.store.has_action(&f.b)?);
    let input =
        json!({"to":f.b,"body":"FYI","reasoning":"Record fact","key":"same","wake":"passive"});
    let first = f.store.call(&f.a, "send_message", &input)?;
    assert_eq!(first, f.store.call(&f.a, "send_message", &input)?);
    let mut other = input;
    other["wake"] = json!("action");
    assert!(f.store.call(&f.a, "send_message", &other).is_err());
    assert!(
        f.sql(
            "UPDATE messages SET wake='action' WHERE id=?",
            &[first["id"].clone()]
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn path_guard_is_read_only_and_checks_live_foreign_tree_leases() -> Result<()> {
    let f = Fixture::new()?;
    let lease = f.store.call(
        &f.a,
        "lock",
        &json!({"path":"src","kind":"tree","reasoning":"Changing module"}),
    )?;
    let paths = json!({"paths":[{"path":"SRC/file.rs"},{"path":"src/other.rs"}]});
    let blocked = f.store.call(&f.b, "check_paths", &paths)?;
    assert_eq!(blocked["ok"], false);
    assert_eq!(
        blocked["conflicts"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Expected array"))?
            .len(),
        1
    );
    assert_eq!(blocked["conflicts"][0]["reasoning"], "Changing module");
    assert_eq!(f.store.call(&f.a, "check_paths", &paths)?["ok"], true);
    f.sql(
        "UPDATE leases SET expiresAt=0 WHERE id=?",
        &[lease["lease"]["id"].clone()],
    )?;
    assert_eq!(f.store.call(&f.b, "check_paths", &paths)?["ok"], true);
    assert!(
        f.store
            .call(
                &f.b,
                "check_paths",
                &json!({"paths":[{"path":"../escape"}]})
            )
            .is_err()
    );
    Ok(())
}
