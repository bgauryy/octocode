//! One bounded host Stop check; never a delivery owner or an automatic ACK.
use crate::{
    database::query,
    store::{Store, now},
};
use anyhow::{Result, bail};
use serde_json::{Value, json};

impl Store {
    pub fn completion_check(&self, session: &str, input: &Value) -> Result<Value> {
        // Do not keep a recipient in a model loop when work is blocked or impossible.
        if input["hook_event_name"] != "Stop" || input["stop_hook_active"] == true {
            return Ok(json!({}));
        }
        let snapshot = self.db.unchecked_transaction()?;
        let identity = self.known(session, true)?;
        if identity["vendorSession"] != input["session_id"]
            || std::fs::canonicalize(input["cwd"].as_str().unwrap_or(""))?
                != std::path::Path::new(&self.workspace)
        {
            bail!("Completion check requires the bound native session and workspace");
        }
        let attachment = self.attachment(session)?;
        if attachment["transport"] != "claude" {
            bail!("Claude completion check requires a Claude native binding");
        }
        // Only already-submitted mail: queued passive mail must not create a turn.
        // This is metadata, not proof the host accepted a socket write.
        let pending = query(
            &snapshot,
            "SELECT m.id FROM deliveries d JOIN messages m ON m.id=d.message JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient WHERE d.recipient=? AND d.acknowledgedAt IS NULL AND m.expiresAt>? AND x.state='submitted' ORDER BY m.id LIMIT 17",
            &[json!(session), json!(now())],
        )?;
        snapshot.commit()?;
        if pending.is_empty() {
            return Ok(json!({}));
        }
        let ids: Vec<_> = pending
            .iter()
            .take(16)
            .map(|row| row["id"].clone())
            .collect();
        Ok(json!({"decision":"block","reason":format!(
            "Peer work is still unacknowledged: IDs {}{}. Handle these IDs from existing context; only if a body is missing, recover it with inbox(message:ID). Reply to requests using replyTo and ackReply:true; batch-ACK handled FYIs/answers. Do not ACK unfinished work or send ACK messages. If genuinely blocked, explain why and stop; this check will not block the recovery turn again.",
            json!(ids), if pending.len()>16 { " (more pending; use inbox recovery as needed)" } else { "" }
        )}))
    }
}
