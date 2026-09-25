use anyhow::{Result, bail};
use rusqlite::{
    Connection, OpenFlags, Transaction, TransactionBehavior, params_from_iter,
    types::{Value as SqlValue, ValueRef},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const VERSION: i64 = 2;
pub const APPLICATION_ID: i64 = 1329678147;
pub const SQL: &str = concat!(include_str!("schema-v1.sql"), include_str!("schema-v2.sql"));

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
                ValueRef::Null => Value::Null,
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
    Ok(db.execute(sql, params_from_iter(values(args)?))?)
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
    let mut rows = query(
        db,
        "SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name",
        &[],
    )?;
    for row in &mut rows {
        row["sql"] = json!(
            row["sql"]
                .as_str()
                .unwrap_or("")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    Ok(serde_json::to_string(&rows)?)
}
pub fn fingerprint(db: &Connection) -> Result<String> {
    Ok(Sha256::digest(signature(db)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn expected() -> Result<String> {
    let db = Connection::open_in_memory()?;
    db.execute_batch(SQL)?;
    fingerprint(&db)
}
fn compatible(db: &Connection) -> Result<bool> {
    let info = metadata(db)?;
    Ok(info["applicationId"] == APPLICATION_ID
        && info["schemaVersion"] == VERSION
        && fingerprint(db)? == expected()?)
}
fn validate(db: &Connection) -> Result<()> {
    if !compatible(db)? {
        bail!(
            "Incompatible agents-communication database: expected schema v2. Stop old workers and use db migrate for an intact v1 store. No automatic repair."
        );
    }
    Ok(())
}
pub fn migrate(path: &Path) -> Result<Value> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.pragma_update(None, "foreign_keys", true)?;
    transaction(&db, |db| {
        if compatible(db)? {
            return Ok(json!({"schemaVersion":VERSION,"migrated":false}));
        }
        let old = Connection::open_in_memory()?;
        old.execute_batch(include_str!("schema-v1.sql"))?;
        let info = metadata(db)?;
        if info["applicationId"] != APPLICATION_ID
            || info["schemaVersion"] != 1
            || fingerprint(db)? != fingerprint(&old)?
        {
            bail!("Migration requires an intact v1 store");
        }
        let active: i64 = db.query_row(
            "SELECT count(*) FROM sessions WHERE expiresAt>?",
            [crate::store::now()],
            |r| r.get(0),
        )?;
        if active != 0 {
            bail!("Stop workers and leave sessions or wait for presence expiry before migration");
        }
        db.execute_batch(include_str!("schema-v2.sql"))?;
        db.execute("INSERT INTO audit(session,kind,entityId,at,data) SELECT id,'session.imported',id,?,json_object('name',name,'vendor',vendor,'vendorSession',vendorSession,'workspace',workspace) FROM sessions", [crate::store::now()])?;
        db.pragma_update(None, "user_version", VERSION)?;
        validate(db)?;
        Ok(json!({"schemaVersion":VERSION,"migrated":true}))
    })
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
        validate(&db)?;
        return Ok(db);
    }
    if rusqlite::version_number() < 3_051_003 {
        bail!("SQLite >=3.51.3 required for concurrent WAL");
    }
    transaction(&db, |db| {
        let info = metadata(db)?;
        let empty = query(db, "SELECT count(*) AS n FROM sqlite_schema", &[])?[0]["n"] == 0;
        if empty && info["applicationId"] == 0 && info["schemaVersion"] == 0 {
            if !create {
                bail!("Database is not initialized. Run join first.");
            }
            db.execute_batch(SQL)?;
            db.pragma_update(None, "application_id", APPLICATION_ID)?;
            db.pragma_update(None, "user_version", VERSION)?;
        } else {
            validate(db)?;
        }
        Ok(())
    })?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "synchronous", "FULL")?;
    Ok(db)
}
