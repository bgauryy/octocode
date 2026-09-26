//! Bounded directory changes derived from sessions; never a second agent registry.
use crate::{
    database::{execute, query, transaction},
    store::{Store, now},
};
use anyhow::Result;
use serde_json::{Value, json};

impl Store {
    fn peer_snapshot(&self, session: &str) -> Result<Value> {
        let rows = query(
            &self.db,
            "SELECT id,name,vendor,task,status FROM sessions WHERE workspace=? AND id<>? AND expiresAt>? ORDER BY id LIMIT 17",
            &[json!(self.workspace), json!(session), json!(now())],
        )?;
        let mut items = Vec::new();
        let mut bytes = 0;
        let mut more = false;
        for row in rows {
            let size = serde_json::to_vec(&row)?.len();
            if items.len() == 16 || (!items.is_empty() && bytes + size > 3000) {
                more = true;
                break;
            }
            bytes += size;
            items.push(row);
        }
        let next = if more {
            items.last().map(|v| v["id"].clone())
        } else {
            None
        };
        let summary = query(
            &self.db,
            "SELECT coalesce((SELECT revision FROM peer_revisions WHERE workspace=?2),0) AS revision,count(*) AS active FROM sessions WHERE expiresAt>?1 AND workspace=?2 AND id<>?3",
            &[json!(now()), json!(self.workspace), json!(session)],
        )?;
        Ok(json!({"items":items,"next":next,"summary":summary[0]}))
    }
    fn saved_peers(&self, session: &str, consumer: &str, generation: &str) -> Result<Value> {
        let rows = query(
            &self.db,
            "SELECT snapshot FROM peer_views WHERE session=? AND consumer=? AND generation=?",
            &[json!(session), json!(consumer), json!(generation)],
        )?;
        rows.first()
            .map(|row| serde_json::from_str(row["snapshot"].as_str().unwrap_or("null")))
            .transpose()
            .map(|value| value.unwrap_or(Value::Null))
            .map_err(Into::into)
    }
    pub(crate) fn peers_changed(
        &self,
        session: &str,
        consumer: &str,
        generation: &str,
    ) -> Result<bool> {
        Ok(self.peer_snapshot(session)? != self.saved_peers(session, consumer, generation)?)
    }
    pub(crate) fn peer_context(
        &self,
        session: &str,
        consumer: &str,
        generation: &str,
    ) -> Result<String> {
        if !self.peers_changed(session, consumer, generation)? {
            return Ok(String::new());
        }
        transaction(&self.db, |db| {
            let snapshot = self.peer_snapshot(session)?;
            let previous = self.saved_peers(session, consumer, generation)?;
            if snapshot == previous {
                return Ok(String::new());
            }
            execute(
                db,
                "INSERT INTO peer_views(session,consumer,generation,snapshot) VALUES(?,?,?,?) ON CONFLICT(session,consumer) DO UPDATE SET generation=excluded.generation,snapshot=excluded.snapshot",
                &[
                    json!(session),
                    json!(consumer),
                    json!(generation),
                    json!(snapshot.to_string()),
                ],
            )?;
            let empty = Vec::new();
            let before = previous["items"].as_array().unwrap_or(&empty);
            let current = snapshot["items"].as_array().unwrap_or(&empty);
            let changed: Vec<_> = current.iter().filter(|row| !before.contains(row)).collect();
            let removed: Vec<_> = before
                .iter()
                .filter(|row| !current.iter().any(|new| new["id"] == row["id"]))
                .map(|row| &row["id"])
                .collect();
            if changed.is_empty()
                && removed.is_empty()
                && snapshot["next"].is_null()
                && previous["next"].is_null()
            {
                return Ok(String::new());
            }
            let mut delta = json!({"upsert":changed,"removedFromView":removed});
            if !snapshot["next"].is_null() {
                delta["next"] = json!({"command":"peers","input":{"after":snapshot["next"]}});
            }
            if !previous["next"].is_null() {
                delta["refresh"] = json!({"command":"peers","input":{}});
            }
            Ok(format!(
                "Peer directory (declared data; use exact IDs; removedFromView is not proof of departure; follow next): {delta}"
            ))
        })
    }
}
