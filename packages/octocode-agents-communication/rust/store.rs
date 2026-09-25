use crate::{
    catalog::{self, text, ttl},
    database::{self, execute, query, transaction},
    paths::{overlap, resolve_path},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub struct Store {
    pub(crate) db: Connection,
    database_identity: fs::Metadata,
    pub workspace: String,
    pub database: PathBuf,
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn page(mut rows: Vec<Value>, string_cursor: bool) -> Value {
    let mut bytes = 64;
    let mut count = 0;
    for row in rows.iter().take(100) {
        let size = row.to_string().len() + 1;
        if count > 0 && bytes + size > 256 * 1024 {
            break;
        }
        bytes += size;
        count += 1;
    }
    let next = if rows.len() > count && count > 0 {
        if string_cursor {
            json!(
                rows[count - 1]["id"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| rows[count - 1]["id"].to_string())
            )
        } else {
            rows[count - 1]["id"].clone()
        }
    } else {
        Value::Null
    };
    rows.truncate(count);
    json!({"items":rows,"next":next})
}
impl Store {
    pub fn open(
        database: PathBuf,
        workspace: &Path,
        read_only: bool,
        create: bool,
    ) -> Result<Self> {
        let workspace = fs::canonicalize(workspace)?
            .to_str()
            .ok_or_else(|| anyhow!("Workspace must be UTF-8"))?
            .to_owned();
        let db = database::open(&database, read_only, create)?;
        let database_identity = fs::metadata(&database)?;
        Ok(Self {
            db,
            database_identity,
            workspace,
            database,
        })
    }
    fn check_database(&self) -> Result<()> {
        let current = fs::metadata(&self.database)
            .map_err(|_| anyhow!("Coordination database disappeared; stop this worker"))?;
        if !current.is_file() || !self.database_identity.is_file() {
            bail!("Coordination database is no longer a file; stop this worker");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (current.dev(), current.ino())
                != (self.database_identity.dev(), self.database_identity.ino())
            {
                bail!("Coordination database was replaced; stop this worker");
            }
        }
        Ok(())
    }
    pub(crate) fn known(&self, id: &str, active: bool) -> Result<Value> {
        self.check_database()?;
        let rows = query(
            &self.db,
            "SELECT * FROM sessions WHERE id=? AND workspace=? AND (?=0 OR expiresAt>?)",
            &[
                json!(id),
                json!(self.workspace),
                json!(active),
                json!(now()),
            ],
        )?;
        rows.into_iter()
            .next()
            .ok_or_else(|| anyhow!("Unknown or expired session in this workspace"))
    }
    pub fn call(&self, session: &str, name: &str, input: &Value) -> Result<Value> {
        self.check_database()?;
        catalog::command(name, input)?;
        match name {
            "peers" => Ok(page(
                query(
                    &self.db,
                    "SELECT * FROM sessions WHERE workspace=? AND expiresAt>? AND id>? ORDER BY id LIMIT 101",
                    &[
                        json!(self.workspace),
                        json!(now()),
                        json!(input["after"].as_str().unwrap_or("")),
                    ],
                )?,
                true,
            )),
            "inbox" => self.inbox(session, input["after"].as_i64().unwrap_or(0)),
            "join" => transaction(&self.db, |db| {
                let id = Uuid::new_v4().to_string();
                let name = text(input, "name")?;
                let vendor = text(input, "vendor")?;
                if input.get("vendorSession").is_some() {
                    text(input, "vendorSession")?;
                }
                let expires = now() + 60_000;
                execute(
                    db,
                    "INSERT INTO sessions(id,workspace,name,vendor,vendorSession,expiresAt) VALUES(?,?,?,?,?,?)",
                    &[
                        json!(id),
                        json!(self.workspace),
                        json!(name),
                        json!(vendor),
                        input["vendorSession"].clone(),
                        json!(expires),
                    ],
                )?;
                Ok(
                    json!({"id":id,"name":name,"vendor":vendor,"vendorSession":input["vendorSession"],"expiresAt":expires}),
                )
            }),
            "resume" => transaction(&self.db, |db| {
                let row = self.known(session, false)?;
                if row["vendor"] != text(input, "vendor")?
                    || row["expiresAt"].as_i64().unwrap_or(0) > now()
                {
                    bail!("Vendor mismatch or session still active");
                }
                execute(db, "DELETE FROM leases WHERE owner=?", &[json!(session)])?;
                execute(
                    db,
                    "UPDATE deliveries SET claimUntil=0,claimedBy=NULL WHERE recipient=? AND acknowledgedAt IS NULL",
                    &[json!(session)],
                )?;
                execute(
                    db,
                    "UPDATE sessions SET expiresAt=? WHERE id=?",
                    &[json!(now() + 60_000), json!(session)],
                )?;
                self.known(session, true)
            }),
            "heartbeat" => transaction(&self.db, |db| {
                self.known(session, true)?;
                if input.get("vendorSession").is_some() {
                    text(input, "vendorSession")?;
                }
                execute(
                    db,
                    "UPDATE sessions SET expiresAt=?,vendorSession=coalesce(?,vendorSession) WHERE id=?",
                    &[
                        json!(now() + 60_000),
                        input["vendorSession"].clone(),
                        json!(session),
                    ],
                )?;
                Ok(json!({"alive":true}))
            }),
            "leave" => transaction(&self.db, |db| {
                self.known(session, false)?;
                execute(db, "DELETE FROM leases WHERE owner=?", &[json!(session)])?;
                execute(
                    db,
                    "UPDATE sessions SET expiresAt=? WHERE id=?",
                    &[json!(now()), json!(session)],
                )?;
                Ok(json!({"left":true}))
            }),
            "subscribe" => transaction(&self.db, |db| {
                self.known(session, true)?;
                let topics = input["topics"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Missing topics"))?;
                execute(
                    db,
                    "DELETE FROM subscriptions WHERE session=?",
                    &[json!(session)],
                )?;
                for topic in topics {
                    text(&json!({"topic":topic}), "topic")?;
                    execute(
                        db,
                        "INSERT OR IGNORE INTO subscriptions VALUES(?,?)",
                        &[json!(session), topic.clone()],
                    )?;
                }
                Ok(json!({"subscribed":true}))
            }),
            "lock" => self.lock(session, input),
            "renew" | "unlock" => transaction(&self.db, |db| {
                self.known(session, true)?;
                let count = if name == "renew" {
                    execute(
                        db,
                        "UPDATE leases SET expiresAt=? WHERE id=? AND owner=? AND expiresAt>?",
                        &[
                            json!(now() + ttl(input, 60_000)?),
                            input["lease"].clone(),
                            json!(session),
                            json!(now()),
                        ],
                    )?
                } else {
                    execute(
                        db,
                        "DELETE FROM leases WHERE id=? AND owner=? AND expiresAt>?",
                        &[input["lease"].clone(), json!(session), json!(now())],
                    )?
                };
                Ok(if name == "renew" {
                    json!({"renewed":count==1})
                } else {
                    json!({"released":count==1})
                })
            }),
            "send_message" => self.send(session, input, false),
            "notify_all" => self.send(session, input, true),
            "ack" => transaction(&self.db, |db| {
                self.known(session, true)?;
                let count = execute(
                    db,
                    "UPDATE deliveries SET acknowledgedAt=coalesce(acknowledgedAt,?) WHERE message=? AND recipient=?",
                    &[json!(now()), input["message"].clone(), json!(session)],
                )?;
                Ok(json!({"acknowledged":count==1}))
            }),
            "prune" => transaction(&self.db, |db| {
                let removed = execute(
                    db,
                    "DELETE FROM leases WHERE id IN (SELECT id FROM leases WHERE expiresAt<=? ORDER BY id LIMIT 100)",
                    &[json!(now())],
                )?;
                Ok(
                    json!({"removed":removed,"morePossible":removed>0,"next":if removed>0 {json!({"command":"prune"})}else{Value::Null}}),
                )
            }),
            _ => bail!("Unknown operation: {name}"),
        }
    }
    fn lock(&self, session: &str, input: &Value) -> Result<Value> {
        let path = resolve_path(Path::new(&self.workspace), text(input, "path")?)?;
        if !path.starts_with(&self.workspace) {
            bail!("Path escapes workspace");
        }
        let path = path.to_str().ok_or_else(|| anyhow!("Path must be UTF-8"))?;
        let kind = input["kind"].as_str().unwrap_or("file");
        let duration = ttl(input, 60_000)?;
        transaction(&self.db, |db| {
            self.known(session, true)?;
            for row in query(
                db,
                "SELECT l.* FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=? AND l.expiresAt>? AND s.expiresAt>?",
                &[json!(self.workspace), json!(now()), json!(now())],
            )? {
                if overlap(
                    row["path"].as_str().unwrap_or(""),
                    row["kind"].as_str().unwrap_or(""),
                    path,
                    kind,
                ) {
                    return Ok(json!({"ok":false,"conflict":row}));
                }
            }
            let expires = now() + duration;
            execute(
                db,
                "INSERT INTO leases(workspace,path,kind,owner,expiresAt) VALUES(?,?,?,?,?)",
                &[
                    json!(self.workspace),
                    json!(path),
                    json!(kind),
                    json!(session),
                    json!(expires),
                ],
            )?;
            Ok(
                json!({"ok":true,"lease":{"id":db.last_insert_rowid(),"path":path,"kind":kind,"owner":session,"expiresAt":expires}}),
            )
        })
    }
    fn send(&self, session: &str, input: &Value, broadcast: bool) -> Result<Value> {
        if !broadcast && input.get("to").is_some() == input.get("topic").is_some() {
            bail!("Supply exactly one of to or topic");
        }
        let target = if broadcast {
            "*"
        } else {
            text(
                input,
                if input.get("to").is_some() {
                    "to"
                } else {
                    "topic"
                },
            )?
        };
        let body = text(input, "body")?;
        let duration = ttl(input, 3_600_000)?;
        let key = if input.get("key").is_some() {
            text(input, "key")?.to_owned()
        } else {
            Uuid::new_v4().to_string()
        };
        transaction(&self.db, |db| {
            self.known(session, true)?;
            if let Some(row) = query(
                db,
                "SELECT * FROM messages WHERE sender=? AND key=?",
                &[json!(session), json!(key)],
            )?
            .first()
            {
                if row["target"] != target || row["body"] != body || row["topic"] != input["topic"]
                {
                    bail!("Message key reused with different content");
                }
                let count = query(
                    db,
                    "SELECT count(*) AS n FROM deliveries WHERE message=?",
                    &[row["id"].clone()],
                )?;
                return Ok(json!({"id":row["id"],"recipients":count[0]["n"]}));
            }
            let recipients = if broadcast {
                query(
                    db,
                    "SELECT id FROM sessions WHERE workspace=? AND expiresAt>? AND id<>?",
                    &[json!(self.workspace), json!(now()), json!(session)],
                )?
            } else if input.get("to").is_some() {
                self.known(target, false)?;
                vec![json!({"id":target})]
            } else {
                query(
                    db,
                    "SELECT s.id FROM sessions s JOIN subscriptions t ON s.id=t.session WHERE t.topic=? AND s.workspace=? AND s.expiresAt>? AND s.id<>?",
                    &[
                        json!(target),
                        json!(self.workspace),
                        json!(now()),
                        json!(session),
                    ],
                )?
            };
            execute(
                db,
                "INSERT INTO messages(sender,target,topic,body,key,expiresAt) VALUES(?,?,?,?,?,?)",
                &[
                    json!(session),
                    json!(target),
                    input["topic"].clone(),
                    json!(body),
                    json!(key),
                    json!(now() + duration),
                ],
            )?;
            let id = db.last_insert_rowid();
            for recipient in &recipients {
                execute(
                    db,
                    "INSERT INTO deliveries(message,recipient) VALUES(?,?)",
                    &[json!(id), recipient["id"].clone()],
                )?;
            }
            Ok(json!({"id":id,"recipients":recipients.len()}))
        })
    }
    pub fn inbox(&self, session: &str, after: i64) -> Result<Value> {
        self.known(session, true)?;
        Ok(page(
            query(
                &self.db,
                "SELECT m.id,m.sender,m.body,m.topic,m.expiresAt FROM messages m JOIN deliveries d ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND m.id>? ORDER BY m.id LIMIT 101",
                &[json!(session), json!(now()), json!(after)],
            )?,
            false,
        ))
    }
    pub fn claim(&self, session: &str, owner: &str) -> Result<Vec<Value>> {
        self.known(session, true)?;
        // Idle workers read under WAL without competing with message/lease writers.
        let pending: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM deliveries d JOIN messages m ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND d.claimUntil<=? AND m.expiresAt>? AND (d.claimedBy IS NULL OR d.claimedBy<>?) AND NOT EXISTS(SELECT 1 FROM dispatches x WHERE x.message=d.message AND x.recipient=d.recipient))",
            rusqlite::params![session, now(), now(), owner], |row| row.get(0),
        )?;
        if !pending {
            return Ok(Vec::new());
        }
        transaction(&self.db, |db| {
            self.known(session, true)?;
            let items = query(
                db,
                "SELECT m.id,m.sender,m.body,m.topic,m.expiresAt FROM messages m JOIN deliveries d ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND d.claimUntil<=? AND m.expiresAt>? AND (d.claimedBy IS NULL OR d.claimedBy<>?) AND NOT EXISTS(SELECT 1 FROM dispatches x WHERE x.message=d.message AND x.recipient=d.recipient) ORDER BY m.id LIMIT 10",
                &[json!(session), json!(now()), json!(now()), json!(owner)],
            )?;
            for item in &items {
                execute(
                    db,
                    "UPDATE deliveries SET claimedBy=?,claimUntil=? WHERE message=? AND recipient=?",
                    &[
                        json!(owner),
                        json!(now() + 30_000),
                        item["id"].clone(),
                        json!(session),
                    ],
                )?;
            }
            Ok(items)
        })
    }
}
