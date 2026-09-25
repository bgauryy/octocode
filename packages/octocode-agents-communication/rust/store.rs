use crate::{
    catalog::{self, text, ttl},
    database::{self, execute, query, transaction},
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
            .ok_or_else(|| anyhow!("Unknown or expired session in this workspace. Refresh peers and copy the exact current ID; never reconstruct it. If your bound identity expired, ask the host to restore it."))
    }
    pub(crate) fn host_identities(&self, vendor: &str, host: &str) -> Result<Vec<Value>> {
        query(
            &self.db,
            "SELECT s.id FROM sessions s LEFT JOIN attachments a ON a.session=s.id WHERE s.workspace=? AND s.vendorSession=? AND (s.vendor=? OR a.transport=?) ORDER BY s.id LIMIT 2",
            &[
                json!(self.workspace),
                json!(host),
                json!(vendor),
                json!(vendor),
            ],
        )
    }
    pub fn call(&self, session: &str, name: &str, input: &Value) -> Result<Value> {
        self.check_database()?;
        catalog::command(name, input)?;
        match name {
            "health" => self.health(input),
            "activity" => {
                self.known(session, false)?;
                crate::activity::read(std::path::Path::new(&self.workspace), input)
            }
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
            "inbox" => {
                if let Some(message) = input.get("message") {
                    self.known(session, true)?;
                    Ok(page(
                        query(
                            &self.db,
                            "SELECT m.id,m.sender,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo FROM messages m JOIN deliveries d ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND m.id=?",
                            &[json!(session), json!(now()), message.clone()],
                        )?,
                        false,
                    ))
                } else {
                    self.inbox(session, input["after"].as_i64().unwrap_or(0))
                }
            }
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
                    self.validate_vendor_session_update(session, &input["vendorSession"])?;
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
            "check_write" => self.check_write(session, input),
            "check_paths" => {
                self.known(session, true)?;
                let mut conflicts = Vec::new();
                for path in input["paths"]
                    .as_array()
                    .ok_or_else(|| anyhow!("paths required"))?
                {
                    let mut filter = path.clone();
                    filter["status"] = json!("active");
                    loop {
                        let rows = self.entity_list(session, "lease", &filter)?;
                        for lease in rows["items"]
                            .as_array()
                            .ok_or_else(|| anyhow!("Invalid lease page"))?
                        {
                            if lease["owner"] != session
                                && !conflicts
                                    .iter()
                                    .any(|prior: &Value| prior["id"] == lease["id"])
                            {
                                conflicts.push(lease.clone());
                            }
                        }
                        if rows["next"].is_null() {
                            break;
                        }
                        filter["after"] = rows["next"].clone();
                    }
                }
                Ok(json!({"ok":conflicts.is_empty(),"conflicts":conflicts}))
            }
            "lock" | "lock_many" => self.lock(session, input, name == "lock_many"),
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
            "share_document" => self.share_document(session, input),
            "read_document" => self.read_document(session, input),
            "notify_all" => self.send(session, input, true),
            "ack" => transaction(&self.db, |db| {
                self.known(session, true)?;
                if let Some(messages) = input["messages"].as_array() {
                    let at = json!(now());
                    for message in messages {
                        let count = execute(
                            db,
                            "UPDATE deliveries SET acknowledgedAt=coalesce(acknowledgedAt,?) WHERE message=? AND recipient=?",
                            &[at.clone(), message.clone(), json!(session)],
                        )?;
                        if count != 1 {
                            // The transaction rolls back earlier updates and audit
                            // triggers, so callers can never receive partial success.
                            bail!(
                                "Batch acknowledgement requires every ID to be received by this session"
                            );
                        }
                    }
                    return Ok(json!({"acknowledged":true,"count":messages.len()}));
                }
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
                    "DELETE FROM leases WHERE id IN (SELECT l.id FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.expiresAt<=? OR s.expiresAt<=? ORDER BY l.id LIMIT 100)",
                    &[json!(now()), json!(now())],
                )?;
                Ok(
                    json!({"removed":removed,"morePossible":removed>0,"next":if removed>0 {json!({"command":"prune"})}else{Value::Null}}),
                )
            }),
            _ => bail!("Unknown operation: {name}"),
        }
    }
    pub(crate) fn validate_vendor_session_update(
        &self,
        session: &str,
        value: &Value,
    ) -> Result<()> {
        if self.known(session, false)?["vendorSession"] != *value {
            self.ensure_binding_change_allowed(session)?;
            if !query(
                &self.db,
                "SELECT 1 FROM attachments WHERE session=? AND transport<>'raw'",
                &[json!(session)],
            )?
            .is_empty()
            {
                bail!("Use attach to change a native receiver's vendorSession");
            }
        }
        Ok(())
    }
    fn send(&self, session: &str, input: &Value, broadcast: bool) -> Result<Value> {
        if !broadcast
            && ((input.get("to").is_some() && input.get("topic").is_some())
                || (input.get("to").is_none()
                    && input.get("topic").is_none()
                    && input.get("replyTo").is_none()))
        {
            bail!("Supply exactly one of to or topic, or replyTo alone");
        }
        let target = if broadcast {
            Some("*")
        } else if input.get("to").is_some() {
            Some(text(input, "to")?)
        } else if input.get("topic").is_some() {
            Some(text(input, "topic")?)
        } else {
            None
        };
        let body = text(input, "body")?;
        let reasoning = text(input, "reasoning")?;
        let ack_reply = input["ackReply"].as_bool().unwrap_or(false);
        if ack_reply
            && (broadcast || input.get("topic").is_some() || input.get("replyTo").is_none())
        {
            bail!("ackReply requires a direct replyTo to an incoming message");
        }
        let duration = ttl(input, 3_600_000)?;
        let wake = input["wake"]
            .as_str()
            .unwrap_or(if broadcast || input.get("topic").is_some() {
                "passive"
            } else {
                "action"
            });
        let key = if input.get("key").is_some() {
            text(input, "key")?.to_owned()
        } else {
            Uuid::new_v4().to_string()
        };
        transaction(&self.db, |db| {
            self.known(session, true)?;
            let mut conversation = input["conversationId"].clone();
            let mut reply_sender = None;
            if let Some(reply) = input.get("replyTo") {
                let parent = query(db,
                    "SELECT m.conversationId,m.sender FROM messages m JOIN sessions s ON s.id=m.sender WHERE m.id=? AND s.workspace=? AND (m.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.recipient=?))",
                    &[reply.clone(), json!(self.workspace), json!(session), json!(session)])?
                    .into_iter().next().ok_or_else(|| anyhow!("Reply requires a visible parent in this workspace"))?;
                if input.get("conversationId").is_some() && conversation != parent["conversationId"]
                {
                    bail!("Reply conversationId must match its parent");
                }
                conversation = parent["conversationId"].clone();
                reply_sender = parent["sender"].as_str().map(str::to_owned);
            }
            let target = target
                .or(reply_sender.as_deref())
                .ok_or_else(|| anyhow!("Reply parent has no sender"))?;
            // Completion is explicit recipient intent, never inferred from visibility,
            // correlation, a transport receipt, or a model turn ending.
            if ack_reply
                && (reply_sender.as_deref() != Some(target)
                    || query(
                        db,
                        "SELECT 1 FROM deliveries WHERE message=? AND recipient=?",
                        &[input["replyTo"].clone(), json!(session)],
                    )?
                    .is_empty())
            {
                bail!(
                    "ackReply requires replying to the sender of a message received by this session"
                );
            }
            let receipt = |id: Value, recipients: Value| -> Result<Value> {
                let mut result = json!({"id":id,"recipients":recipients});
                if ack_reply {
                    execute(
                        db,
                        "UPDATE deliveries SET acknowledgedAt=? WHERE message=? AND recipient=? AND acknowledgedAt IS NULL",
                        &[json!(now()), input["replyTo"].clone(), json!(session)],
                    )?;
                    result["acknowledged"] = json!(true);
                }
                Ok(result)
            };
            if let Some(row) = query(
                db,
                "SELECT * FROM messages WHERE sender=? AND key=?",
                &[json!(session), json!(key)],
            )?
            .first()
            {
                if row["target"] != target
                    || row["body"] != body
                    || row["topic"] != input["topic"]
                    || row["reasoning"] != reasoning
                    || row["wake"] != wake
                    || row["conversationId"] != conversation
                    || row["replyTo"] != input["replyTo"]
                {
                    bail!("Message key reused with different content");
                }
                let count = query(
                    db,
                    "SELECT count(*) AS n FROM deliveries WHERE message=?",
                    &[row["id"].clone()],
                )?;
                return receipt(row["id"].clone(), count[0]["n"].clone());
            }
            let recipients = if broadcast {
                query(
                    db,
                    "SELECT id FROM sessions WHERE workspace=? AND expiresAt>? AND id<>?",
                    &[json!(self.workspace), json!(now()), json!(session)],
                )?
            } else if input.get("topic").is_none() {
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
                "INSERT INTO messages(sender,target,topic,body,key,expiresAt,reasoning,wake,conversationId,replyTo) VALUES(?,?,?,?,?,?,?,?,?,?)",
                &[
                    json!(session),
                    json!(target),
                    input["topic"].clone(),
                    json!(body),
                    json!(key),
                    json!(now() + duration),
                    json!(reasoning),
                    json!(wake),
                    conversation,
                    input["replyTo"].clone(),
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
            receipt(json!(id), json!(recipients.len()))
        })
    }
    pub fn inbox(&self, session: &str, after: i64) -> Result<Value> {
        self.known(session, true)?;
        Ok(page(
            query(
                &self.db,
                "SELECT m.id,m.sender,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo FROM messages m JOIN deliveries d ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND m.id>? ORDER BY m.id LIMIT 101",
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
                "SELECT m.id,m.sender,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo FROM messages m JOIN deliveries d ON m.id=d.message WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND d.claimUntil<=? AND m.expiresAt>? AND (d.claimedBy IS NULL OR d.claimedBy<>?) AND NOT EXISTS(SELECT 1 FROM dispatches x WHERE x.message=d.message AND x.recipient=d.recipient) ORDER BY m.id LIMIT 10",
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
