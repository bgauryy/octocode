//! Explicit maintenance preserves protocol history; expiry is not deletion authority.
use crate::database;
use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

fn integer(input: &Value, name: &str, default: i64) -> i64 {
    // The catalog bounds these to exact JSON-safe integers; accept e.g. 1.0 too.
    input[name].as_f64().map_or(default, |number| number as i64)
}

fn storage(db: &Connection, path: &Path) -> Result<Value> {
    let page_size: i64 = db.pragma_query_value(None, "page_size", |r| r.get(0))?;
    let page_count: i64 = db.pragma_query_value(None, "page_count", |r| r.get(0))?;
    let free_pages: i64 = db.pragma_query_value(None, "freelist_count", |r| r.get(0))?;
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let wal_bytes = match fs::metadata(Path::new(&wal)) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    Ok(
        json!({"pageSize":page_size,"pageCount":page_count,"freePages":free_pages,
        "logicalBytes":page_size.saturating_mul(page_count),
        "reusableBytes":page_size.saturating_mul(free_pages),
        "mainFileBytes":fs::metadata(path)?.len(),"walBytes":wal_bytes}),
    )
}

pub fn report(path: &Path, input: &Value) -> Result<Value> {
    crate::catalog::command("db retention", input)?;
    let now = crate::store::now();
    let before = integer(input, "before", now.saturating_sub(30 * 86400000));
    if before > now {
        bail!("before must be no later than now");
    }
    let after_id = integer(input, "afterId", 0);
    let limit = integer(input, "limit", 100);
    let db = database::open(path, true, false)?;
    let transaction = db.unchecked_transaction()?;
    let mut rows = database::query(&transaction,
        "SELECT m.id,m.expiresAt,length(CAST(m.body AS BLOB)) AS bodyBytes,
         EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.acknowledgedAt IS NULL) AS unacknowledged,
         EXISTS(SELECT 1 FROM dispatches x WHERE x.message=m.id AND x.state IN ('staged','uncertain')) AS unresolved
         FROM messages m WHERE m.id>? ORDER BY m.id LIMIT ?", &[json!(after_id),json!(limit+1)])?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let mut expired_settled = 0u64;
    let mut unacknowledged = 0u64;
    let mut unresolved = 0u64;
    let mut body_bytes = 0u64;
    for row in &rows {
        let pending = row["unacknowledged"] == 1;
        let uncertain = row["unresolved"] == 1;
        unacknowledged += u64::from(pending);
        unresolved += u64::from(uncertain);
        if row["expiresAt"].as_i64().is_some_and(|at| at <= before) && !pending && !uncertain {
            expired_settled += 1;
            body_bytes += row["bodyBytes"].as_u64().unwrap_or(0);
        }
    }
    let next = if has_more {
        rows.last().map(|row| json!({"command":"db retention","input":{"before":before,"afterId":row["id"],"limit":limit}}))
    } else {
        None
    };
    let result = json!({"scope":"all-workspaces","readOnly":true,"deletionSupported":false,
        "before":before,"cutoffField":"messages.expiresAt","observedAt":now,
        "storage":storage(&transaction,path)?,
        "page":{"afterId":after_id,"limit":limit,"count":rows.len(),"hasMore":has_more,
            "expiredSettledMessages":expired_settled,"expiredSettledBodyBytes":body_bytes,
            "unacknowledgedMessages":unacknowledged,"unresolvedDispatchMessages":unresolved},
        "next":next,
        "retainedBecause":["Audit is append-only and does not duplicate message bodies.",
          "Message keys preserve idempotency; replyTo and delivery records preserve correlation and visibility.",
          "Expiry and acknowledgement do not authorize deletion; pending and uncertain attempts remain recoverable."],
        "maintenance":{"compact":"db compact {} reclaims reusable SQLite pages without deleting history.",
          "archive":"db export creates a verified full snapshot; preserve workspace documents separately."}});
    transaction.commit()?;
    Ok(result)
}

pub fn compact(path: &Path) -> Result<Value> {
    let initial = fs::symlink_metadata(path)?;
    if !initial.file_type().is_file() {
        bail!("Compaction requires a regular database file, not a symlink");
    }
    let db = database::open(path, false, false)?;
    db.busy_timeout(Duration::from_secs(2))?;
    let fingerprint = database::fingerprint(&db)?;
    let before = storage(&db, path)?;
    db.execute_batch("VACUUM")
        .context("Compaction failed; no automatic retry was attempted")?;
    if database::fingerprint(&db)? != fingerprint {
        bail!("Compaction completed but schema verification failed");
    }
    let integrity = database::query(&db, "PRAGMA integrity_check", &[])?;
    if integrity.len() != 1
        || integrity[0]["integrity_check"] != "ok"
        || !database::query(&db, "PRAGMA foreign_key_check", &[])?.is_empty()
    {
        bail!("Compaction completed but database integrity verification failed");
    }
    // PASSIVE never waits for active readers to release their snapshots.
    let checkpoint = database::query(&db, "PRAGMA wal_checkpoint(PASSIVE)", &[])?;
    let current = fs::symlink_metadata(path)?;
    if !current.file_type().is_file() {
        bail!("Database path was replaced during compaction");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (initial.dev(), initial.ino()) != (current.dev(), current.ino()) {
            bail!("Database path was replaced during compaction");
        }
    }
    let after = storage(&db, path)?;
    Ok(
        json!({"scope":"all-workspaces","compacted":true,"deletedRecords":0,
        "schemaVersion":database::VERSION,"schemaSha256":fingerprint,"integrity":"ok",
        "reclaimedBytes":before["mainFileBytes"].as_u64().unwrap_or(0).saturating_sub(after["mainFileBytes"].as_u64().unwrap_or(0)),
        "logicalReclaimedBytes":before["logicalBytes"].as_u64().unwrap_or(0).saturating_sub(after["logicalBytes"].as_u64().unwrap_or(0)),
        "before":before,"after":after,"checkpoint":checkpoint,
        "measurement":"reclaimedBytes is the nonnegative main-file size difference; WAL allocation and concurrent writers can mask savings. No WAL truncation is forced."}),
    )
}
