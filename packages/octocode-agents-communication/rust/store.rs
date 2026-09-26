use crate::{
    catalog::{self, text, ttl},
    database::{self, execute, query, transaction},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
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
    writer: bool,
    /// Last empty inbox read: (session, after, data_version, total_changes).
    idle_inbox: RefCell<Option<(String, i64, i64, u64)>>,
}
impl Drop for Store {
    fn drop(&mut self) {
        // Keep planner statistics current for new indexes; bounded and advisory.
        if self.writer {
            let _ = self.db.execute_batch("PRAGMA optimize");
        }
    }
}
/// Remove null members recursively so outputs never carry empty optional fields.
pub fn strip_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(strip_nulls);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}
struct ByteCount(usize);
impl std::io::Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
        // Measure the serialized size without allocating a string per row.
        let mut counter = ByteCount(1);
        if serde_json::to_writer(&mut counter, row).is_err() {
            break;
        }
        let size = counter.0;
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
            writer: !read_only,
            idle_inbox: RefCell::new(None),
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
                    "SELECT id,name,vendor,vendorSession,expiresAt FROM sessions WHERE workspace=? AND expiresAt>? AND id>? ORDER BY id LIMIT 101",
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
                            "SELECT m.id,m.sender,s.name AS senderName,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo FROM deliveries d JOIN messages m ON m.id=d.message JOIN sessions s ON s.id=m.sender WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND m.id=?",
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
                let identity = self.known(session, true)?;
                if input.get("vendorSession").is_some() {
                    text(input, "vendorSession")?;
                    self.validate_vendor_session_update(
                        session,
                        &identity,
                        &input["vendorSession"],
                    )?;
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
                // Leaving ends the identity's claims; a crash-resume keeps subscriptions.
                execute(db, "DELETE FROM leases WHERE owner=?", &[json!(session)])?;
                execute(
                    db,
                    "DELETE FROM subscriptions WHERE session=?",
                    &[json!(session)],
                )?;
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
                for topic in topics {
                    text(&json!({"topic":topic}), "topic")?;
                }
                // Apply only the difference: unchanged topics write no audit rows.
                execute(
                    db,
                    "DELETE FROM subscriptions WHERE session=? AND topic NOT IN (SELECT value FROM json_each(?))",
                    &[
                        json!(session),
                        json!(Value::Array(topics.clone()).to_string()),
                    ],
                )?;
                for topic in topics {
                    execute(
                        db,
                        "INSERT OR IGNORE INTO subscriptions VALUES(?,?)",
                        &[json!(session), topic.clone()],
                    )?;
                }
                Ok(json!({"subscribed":true}))
            }),
            "check_write" => self.check_write(session, input),
            "check_paths" => self.check_paths(session, input),
            "lock" | "lock_many" => self.lock(session, input, name == "lock_many"),
            "renew" | "unlock" => self.lease_transition(session, input, name == "renew"),
            "send_message" => self.send(session, input, false),
            "share_document" => self.share_document(session, input),
            "read_document" => self.read_document(session, input),
            "context" => self.context(session, input),
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
            // Scoped to the bound workspace: one workspace never deletes another's leases.
            "prune" => transaction(&self.db, |db| {
                let at = now();
                let removed = execute(
                    db,
                    "DELETE FROM leases WHERE id IN (SELECT l.id FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=? AND (l.expiresAt<=? OR s.expiresAt<=?) ORDER BY l.id LIMIT 100)",
                    &[json!(self.workspace), json!(at), json!(at)],
                )?;
                let next = if removed == 100 {
                    json!({"command":"prune"})
                } else {
                    Value::Null
                };
                Ok(json!({"removed":removed,"next":next}))
            }),
            _ => bail!("Unknown operation: {name}"),
        }
    }
    pub(crate) fn validate_vendor_session_update(
        &self,
        session: &str,
        identity: &Value,
        value: &Value,
    ) -> Result<()> {
        if identity["vendorSession"] != *value {
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
                    || row.get("ttlMs").is_some_and(|stored| *stored != duration)
                {
                    bail!(
                        "Message key reused with different content (target, topic, body, reasoning, wake, correlation or ttlMs); use a new key"
                    );
                }
                let count = query(
                    db,
                    "SELECT count(*) AS n FROM deliveries WHERE message=?",
                    &[row["id"].clone()],
                )?;
                return receipt(row["id"].clone(), count[0]["n"].clone());
            }
            let mut offline = false;
            let recipients = if broadcast {
                query(
                    db,
                    "SELECT id FROM sessions WHERE workspace=? AND expiresAt>? AND id<>?",
                    &[json!(self.workspace), json!(now()), json!(session)],
                )?
            } else if input.get("topic").is_none() {
                // Offline direct targets stay valid (resume delivers); flag it to the sender.
                offline = self.known(target, false)?["expiresAt"]
                    .as_i64()
                    .is_none_or(|at| at <= now());
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
                "INSERT INTO messages(sender,target,topic,body,key,expiresAt,reasoning,wake,conversationId,replyTo,ttlMs) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
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
                    json!(duration),
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
            let mut result = receipt(json!(id), json!(recipients.len()))?;
            if offline {
                result["recipientOffline"] = json!(true);
            }
            Ok(result)
        })
    }
    pub fn inbox(&self, session: &str, after: i64) -> Result<Value> {
        self.known(session, true)?;
        // An empty page stays empty until some commit lands: skip the query while
        // neither another connection (data_version) nor this one changed the store.
        let version: i64 = self
            .db
            .pragma_query_value(None, "data_version", |r| r.get(0))?;
        let changes = self.db.total_changes();
        let state = (session.to_owned(), after, version, changes);
        if self.idle_inbox.borrow().as_ref() == Some(&state) {
            return Ok(json!({"items":[],"next":null}));
        }
        let result = page(
            query(
                &self.db,
                "SELECT m.id,m.sender,s.name AS senderName,m.body,m.reasoning,m.topic,m.expiresAt,m.wake,m.conversationId,m.replyTo FROM deliveries d JOIN messages m ON m.id=d.message JOIN sessions s ON s.id=m.sender WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND d.message>? ORDER BY d.message LIMIT 101",
                &[json!(session), json!(now()), json!(after)],
            )?,
            false,
        );
        *self.idle_inbox.borrow_mut() = result["items"]
            .as_array()
            .is_some_and(Vec::is_empty)
            .then_some(state);
        Ok(result)
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
