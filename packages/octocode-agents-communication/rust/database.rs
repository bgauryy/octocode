use anyhow::{Result, bail};
use rusqlite::{
    Connection, OpenFlags, Transaction, TransactionBehavior, params_from_iter,
    types::{Value as SqlValue, ValueRef},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

pub const VERSION: i64 = 7;
pub const APPLICATION_ID: i64 = 1329678147;
/// Schema steps; `STEPS[n]` upgrades an intact v`n` store to v`n+1`.
const STEPS: [&str; 7] = [
    include_str!("schema-v1.sql"),
    include_str!("schema-v2.sql"),
    include_str!("schema-v3.sql"),
    include_str!("schema-v4.sql"),
    include_str!("schema-v5.sql"),
    include_str!("schema-v6.sql"),
    include_str!("schema-v7.sql"),
];
pub const SQL: &str = concat!(
    include_str!("schema-v1.sql"),
    include_str!("schema-v2.sql"),
    include_str!("schema-v3.sql"),
    include_str!("schema-v4.sql"),
    include_str!("schema-v5.sql"),
    include_str!("schema-v6.sql"),
    include_str!("schema-v7.sql")
);

pub fn values(args: &[Value]) -> Result<Vec<SqlValue>> {
    args.iter()
        .map(|v| {
            Ok(match v {
                Value::Null => SqlValue::Null,
                Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
                Value::String(s) => SqlValue::Text(s.clone()),
                Value::Number(n) => SqlValue::Integer(
                    n.as_i64()
                        .ok_or_else(|| anyhow::anyhow!("Expected integer"))?,
                ),
                _ => bail!("Unsupported SQL argument"),
            })
        })
        .collect()
}
/// Rows are JSON objects without SQL NULL members: absent means null, so outputs
/// never spend bytes on empty optional fields.
pub fn query(db: &Connection, sql: &str, args: &[Value]) -> Result<Vec<Value>> {
    let mut statement = db.prepare(sql)?;
    let columns: Vec<String> = statement
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let bindings = values(args)?;
    let mut rows = statement.query(params_from_iter(bindings))?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let mut object = Map::new();
        for (i, name) in columns.iter().enumerate() {
            let value = match row.get_ref(i)? {
                ValueRef::Null => continue,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(s) => json!(std::str::from_utf8(s)?),
                ValueRef::Blob(_) => bail!("Unexpected blob"),
            };
            object.insert(name.clone(), value);
        }
        result.push(Value::Object(object));
    }
    Ok(result)
}
pub fn execute(db: &Connection, sql: &str, args: &[Value]) -> Result<usize> {
    Ok(db.prepare(sql)?.execute(params_from_iter(values(args)?))?)
}
/// Deferred read transaction: validation and reads never take the writer lock.
pub fn read_transaction<T>(
    db: &Connection,
    action: impl FnOnce(&Connection) -> Result<T>,
) -> Result<T> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Deferred)?;
    let result = action(&tx)?;
    tx.commit()?;
    Ok(result)
}
pub fn transaction<T>(db: &Connection, action: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let result = action(&tx)?;
    tx.commit()?;
    Ok(result)
}
pub fn metadata(db: &Connection) -> Result<Value> {
    Ok(
        json!({"applicationId":db.pragma_query_value(None,"application_id",|r| r.get::<_,i64>(0))?,
        "schemaVersion":db.pragma_query_value(None,"user_version",|r| r.get::<_,i64>(0))?,
        "sqliteVersion":rusqlite::version(), "journalMode":db.pragma_query_value(None,"journal_mode",|r| r.get::<_,String>(0))?}),
    )
}
fn signature(db: &Connection) -> Result<String> {
    let mut statement = db.prepare(
        "SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name",
    )?;
    let mut rows = statement.query([])?;
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        let sql: Option<String> = row.get(3)?;
        // The digest format keeps every member, including an empty `sql`.
        result.push(
            json!({"type":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,
            "tbl_name":row.get::<_,String>(2)?,
            "sql":sql.unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ")}),
        );
    }
    Ok(serde_json::to_string(&result)?)
}
pub fn fingerprint(db: &Connection) -> Result<String> {
    Ok(Sha256::digest(signature(db)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn expected() -> Result<String> {
    static EXPECTED: OnceLock<Result<String, String>> = OnceLock::new();
    EXPECTED
        .get_or_init(|| {
            let compute = || -> Result<String> {
                let db = Connection::open_in_memory()?;
                db.execute_batch(SQL)?;
                fingerprint(&db)
            };
            compute().map_err(|error| error.to_string())
        })
        .as_ref()
        .cloned()
        .map_err(|error| anyhow::anyhow!(error.clone()))
}
fn compatible_with(db: &Connection, info: &Value) -> Result<bool> {
    Ok(info["applicationId"] == APPLICATION_ID
        && info["schemaVersion"] == VERSION
        && fingerprint(db)? == expected()?)
}
fn compatible(db: &Connection) -> Result<bool> {
    compatible_with(db, &metadata(db)?)
}
fn validate_with(db: &Connection, info: &Value) -> Result<()> {
    if !compatible_with(db, info)? {
        bail!(
            "Incompatible agents-communication database: expected schema v7. Stop old workers and use db migrate for an intact v1-v6 store. No automatic repair."
        );
    }
    Ok(())
}
fn validate(db: &Connection) -> Result<()> {
    validate_with(db, &metadata(db)?)
}
/// SQL cannot fold Unicode case, so migration computes lease keys natively.
fn backfill_lease_keys(db: &Connection) -> Result<usize> {
    let rows = query(db, "SELECT id,path FROM leases WHERE pathKey IS NULL", &[])?;
    for row in &rows {
        execute(
            db,
            "UPDATE leases SET pathKey=? WHERE id=?",
            &[
                json!(crate::paths::lease_key(row["path"].as_str().unwrap_or(""))),
                row["id"].clone(),
            ],
        )?;
    }
    Ok(rows.len())
}
pub fn migrate(path: &Path) -> Result<Value> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.pragma_update(None, "foreign_keys", true)?;
    let result = transaction(&db, |db| {
        let info = metadata(db)?;
        if compatible_with(db, &info)? {
            return Ok(json!({"schemaVersion":VERSION,"migrated":false}));
        }
        let version = info["schemaVersion"].as_i64().unwrap_or(0);
        let intact =
            info["applicationId"] == APPLICATION_ID && (1..VERSION).contains(&version) && {
                let old = Connection::open_in_memory()?;
                for step in &STEPS[..version as usize] {
                    old.execute_batch(step)?;
                }
                fingerprint(db)? == fingerprint(&old)?
            };
        if !intact {
            bail!("Migration requires an intact v1-v6 store");
        }
        let active: i64 = db.query_row(
            "SELECT count(*) FROM sessions WHERE expiresAt>?",
            [crate::store::now()],
            |r| r.get(0),
        )?;
        if active != 0 {
            bail!("Stop workers and leave sessions or wait for presence expiry before migration");
        }
        for (index, step) in STEPS.iter().enumerate().skip(version as usize) {
            db.execute_batch(step)?;
            if index == 1 {
                db.execute("INSERT INTO audit(session,kind,entityId,at,data) SELECT id,'session.imported',id,?,json_object('name',name,'vendor',vendor,'vendorSession',vendorSession,'workspace',workspace) FROM sessions", [crate::store::now()])?;
            }
        }
        backfill_lease_keys(db)?;
        db.pragma_update(None, "user_version", VERSION)?;
        validate(db)?;
        Ok(json!({"schemaVersion":VERSION,"migrated":true}))
    })?;
    // New indexes need planner statistics; bounded analysis, advisory on failure.
    let _ = db.execute_batch("PRAGMA optimize=0x10002");
    Ok(result)
}
pub fn path(database: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = database {
        return Ok(std::path::absolute(path)?);
    }
    let cwd = std::env::current_dir()?;
    let home = std::env::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot locate home; supply --database"))?;
    Ok(
        crate::home::octocode_home(std::env::var("OCTOCODE_HOME").ok().as_deref(), &cwd, &home)
            .join("agents-communication/v1.sqlite"),
    )
}
pub fn inspect(path: &Path, workspace: &Path) -> Result<Value> {
    let mut result = json!({"path":path,"workspace":fs::canonicalize(workspace)?,"exists":path.exists(),
        "expectedApplicationId":APPLICATION_ID,"expectedSchemaVersion":VERSION,"expectedSchemaSha256":expected()?,"compatible":false});
    if path.exists() {
        let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if let Some(fields) = metadata(&db)?.as_object() {
            for (key, value) in fields {
                result[key] = value.clone();
            }
        }
        result["schemaSha256"] = json!(fingerprint(&db)?);
        result["compatible"] = json!(compatible(&db)?);
    }
    Ok(result)
}
/// Export logical content, including committed WAL pages, without changing the source.
/// A complete verified file is linked into place; the requested path is never a partial file.
pub fn export(source: &Path, destination: &Path) -> Result<Value> {
    if !destination.is_absolute() {
        bail!("Export path must be absolute");
    }
    let filename = destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Export path must name a new file"))?;
    let parent = fs::canonicalize(
        destination
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Export parent is required"))?,
    )?;
    let destination = parent.join(filename);
    match fs::symlink_metadata(&destination) {
        Ok(_) => bail!("Export destination already exists; never overwrite a file or symlink"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let source_metadata = fs::symlink_metadata(source)?;
    if !source_metadata.file_type().is_file() {
        bail!("Export source must be a regular database file, not a symlink");
    }
    let source = fs::canonicalize(source)?;
    let db = open(&source, true, false)?;
    let temporary = tempfile::Builder::new()
        .prefix(".communication-export-")
        .tempfile_in(&parent)?;
    // NamedTempFile creates owner-only files on Unix; keep the destination equally private.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    db.execute(
        "VACUUM main INTO ?",
        [temporary
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Export path must be UTF-8"))?],
    )?;
    let current_metadata = fs::symlink_metadata(&source)?;
    if !current_metadata.file_type().is_file() {
        bail!("Source database was replaced during export");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (source_metadata.dev(), source_metadata.ino())
            != (current_metadata.dev(), current_metadata.ino())
        {
            bail!("Source database was replaced during export");
        }
    }
    let snapshot = open(temporary.path(), true, false)?;
    let integrity = query(&snapshot, "PRAGMA integrity_check", &[])?;
    if integrity.len() != 1 || integrity[0]["integrity_check"] != "ok" {
        bail!("Export integrity check failed");
    }
    if !query(&snapshot, "PRAGMA foreign_key_check", &[])?.is_empty() {
        bail!("Export foreign key check failed");
    }
    drop(snapshot);
    temporary.as_file().sync_all()?;
    let bytes = temporary.as_file().metadata()?.len();
    let mut hash = Sha256::new();
    let mut reader = temporary.as_file();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let sha256 = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    // Same-directory hard-link publication is no-clobber, including a destination created
    // after the preflight check. Drop removes only the temporary name, not the snapshot.
    fs::hard_link(temporary.path(), &destination)?;
    drop(temporary);
    // Once published, a directory-sync limitation must be reported, not hide a valid artifact.
    let directory_synced = fs::File::open(&parent)
        .and_then(|directory| directory.sync_all())
        .is_ok();
    Ok(
        json!({"path":destination,"source":source,"schemaVersion":VERSION,"sha256":sha256,
        "bytes":bytes,"scope":"all-workspaces","includesWorkspaceDocuments":false,
        "documents":"Preserve referenced workspace .octocode/communication files separately; this snapshot contains their audit metadata only.",
        "integrity":"ok","directorySynced":directory_synced}),
    )
}
pub fn open(path: &Path, read_only: bool, create: bool) -> Result<Connection> {
    if !path.exists() {
        if read_only || !create {
            bail!(
                "Database does not exist: {}. Run join first.",
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(parent)?;
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        if let Err(error) = options.open(path)
            && error.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(error.into());
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match open_connection(path, read_only, create) {
            Ok(db) => return Ok(db),
            Err(error) => {
                let busy = matches!(error.downcast_ref::<rusqlite::Error>(),
                    Some(rusqlite::Error::SqliteFailure(code, _)) if matches!(code.code,
                        rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked));
                if !busy || std::time::Instant::now() >= deadline {
                    return Err(error);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
fn open_connection(path: &Path, read_only: bool, create: bool) -> Result<Connection> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let db = Connection::open_with_flags(path, flags)?;
    db.create_scalar_function(
        "lease_overlap",
        4,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            Ok(crate::paths::overlap(
                &ctx.get::<String>(0)?,
                &ctx.get::<String>(1)?,
                &ctx.get::<String>(2)?,
                &ctx.get::<String>(3)?,
            ))
        },
    )?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.pragma_update(None, "foreign_keys", true)?;
    if read_only {
        read_transaction(&db, validate)?;
        return Ok(db);
    }
    if rusqlite::version_number() < 3_051_003 {
        bail!("SQLite >=3.51.3 required for concurrent WAL");
    }
    // Validate under a read snapshot; only first initialization takes the writer lock.
    let uninitialized = |info: &Value| info["applicationId"] == 0 && info["schemaVersion"] == 0;
    let info = read_transaction(&db, |db| {
        let info = metadata(db)?;
        if !uninitialized(&info) {
            validate_with(db, &info)?;
        }
        Ok(info)
    })?;
    if uninitialized(&info) {
        transaction(&db, |db| {
            let info = metadata(db)?;
            let empty: i64 =
                db.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))?;
            if empty == 0 && uninitialized(&info) {
                if !create {
                    bail!("Database is not initialized. Run join first.");
                }
                db.execute_batch(SQL)?;
                db.pragma_update(None, "application_id", APPLICATION_ID)?;
                db.pragma_update(None, "user_version", VERSION)?;
            } else {
                validate_with(db, &info)?;
            }
            Ok(())
        })?;
    }
    // journal_mode persists in the file; synchronous is per connection.
    if info["journalMode"] != "wal" {
        db.pragma_update(None, "journal_mode", "WAL")?;
    }
    db.pragma_update(None, "synchronous", "FULL")?;
    Ok(db)
}
