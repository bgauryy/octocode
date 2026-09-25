use crate::{
    catalog,
    database::{execute, query, transaction},
    paths::resolve_path,
    store::{Store, now, page},
};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::Path;

impl Store {
    fn entity_query(&self, session: &str, name: &str) -> Result<(String, Vec<Value>)> {
        self.known(session, false)?;
        let (sql, args) = match name {
            "session" => (
                "SELECT *,expiresAt>? AS active FROM sessions WHERE workspace=?",
                vec![json!(now()), json!(self.workspace)],
            ),
            "lease" => (
                "SELECT l.*,l.expiresAt>? AND s.expiresAt>? AS active FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=? AND s.workspace=?",
                vec![
                    json!(now()),
                    json!(now()),
                    json!(self.workspace),
                    json!(self.workspace),
                ],
            ),
            "message" => (
                "SELECT m.* FROM messages m JOIN sessions s ON s.id=m.sender WHERE s.workspace=? AND (m.sender=? OR EXISTS(SELECT 1 FROM deliveries d WHERE d.message=m.id AND d.recipient=?))",
                vec![json!(self.workspace), json!(session), json!(session)],
            ),
            "delivery" => (
                "SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.* FROM deliveries d JOIN messages m ON m.id=d.message JOIN sessions s ON s.id=m.sender WHERE s.workspace=? AND (m.sender=? OR d.recipient=?)",
                vec![json!(self.workspace), json!(session), json!(session)],
            ),
            "subscriptions" => (
                "SELECT s.id,s.id AS session,coalesce((SELECT json_group_array(topic) FROM (SELECT topic FROM subscriptions WHERE session=s.id ORDER BY topic)), '[]') AS topics FROM sessions s WHERE s.workspace=?",
                vec![json!(self.workspace)],
            ),
            "attachment" => (
                "SELECT a.session AS id,a.* FROM attachments a JOIN sessions s ON s.id=a.session WHERE s.workspace=?",
                vec![json!(self.workspace)],
            ),
            "dispatch" => (
                "SELECT CAST(x.message AS TEXT)||':'||x.recipient AS id,x.* FROM dispatches x JOIN messages m ON m.id=x.message JOIN sessions s ON s.id=m.sender WHERE s.workspace=? AND (m.sender=? OR x.recipient=?)",
                vec![json!(self.workspace), json!(session), json!(session)],
            ),
            "audit" => (
                "SELECT a.* FROM audit a JOIN sessions s ON s.id=a.session WHERE s.workspace=?",
                vec![json!(self.workspace)],
            ),
            _ => bail!("Unknown entity: {name}"),
        };
        Ok((sql.into(), args))
    }
    pub fn entity_get(&self, session: &str, name: &str, id: &str) -> Result<Value> {
        let (sql, mut args) = self.entity_query(session, name)?;
        args.push(json!(id));
        let row = query(
            &self.db,
            &format!("SELECT * FROM ({sql}) WHERE id=?"),
            &args,
        )?
        .into_iter()
        .next()
        .unwrap_or(Value::Null);
        decode(name, row)
    }
    pub fn entity_list(&self, session: &str, name: &str, filters: &Value) -> Result<Value> {
        catalog::validate(&catalog::entity(name)?["list"], filters)?;
        let (sql, mut args) = self.entity_query(session, name)?;
        let mut conditions = Vec::<String>::new();
        if let Some(value) = filters.get("after") {
            conditions.push("id>?".into());
            args.push(value.clone());
        }
        if matches!(name, "session" | "lease") {
            let status = filters["status"].as_str().unwrap_or("active");
            if status != "all" {
                conditions.push("active=?".into());
                args.push(json!(status == "active"));
            }
        }
        for field in ["vendor", "owner", "message", "conversationId", "replyTo"] {
            if let Some(value) = filters.get(field) {
                conditions.push(format!("{field}=?"));
                args.push(value.clone());
            }
        }
        if filters.get("kind").is_some() && filters.get("path").is_none() {
            bail!("kind requires a path conflict query");
        }
        if let Some(path) = filters["path"].as_str() {
            let path = resolve_path(Path::new(&self.workspace), path)?;
            if !path.starts_with(&self.workspace) {
                bail!("Path escapes workspace");
            }
            let path = path.to_str().ok_or_else(|| anyhow!("Path must be UTF-8"))?;
            conditions.push("lease_overlap(path,kind,?,?)".into());
            args.extend([
                json!(path),
                json!(filters["kind"].as_str().unwrap_or("file")),
            ]);
        }
        match filters["direction"].as_str() {
            Some("sent") => {
                conditions.push("sender=?".into());
                args.push(json!(session));
            }
            Some("received") => {
                conditions.push(
                    "EXISTS(SELECT 1 FROM deliveries d WHERE d.message=e.id AND d.recipient=?)"
                        .into(),
                );
                args.push(json!(session));
            }
            _ => {}
        }
        if let Some(topic) = filters.get("topic") {
            conditions.push(
                if name == "subscriptions" {
                    "EXISTS(SELECT 1 FROM subscriptions t WHERE t.session=e.id AND t.topic=?)"
                } else {
                    "topic=?"
                }
                .into(),
            );
            args.push(topic.clone());
        }
        if let Some(ack) = filters["acknowledged"].as_bool() {
            conditions.push(format!(
                "acknowledgedAt IS {}NULL",
                if ack { "NOT " } else { "" }
            ));
        }
        let clauses = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let rows = query(
            &self.db,
            &format!("SELECT * FROM ({sql}) e {clauses} ORDER BY id LIMIT 101"),
            &args,
        )?;
        Ok(page(
            rows.into_iter()
                .map(|row| decode(name, row))
                .collect::<Result<Vec<_>>>()?,
            true,
        ))
    }
    pub fn entity_set(&self, session: &str, name: &str, id: &str, input: &Value) -> Result<Value> {
        if id != session {
            bail!("Only the bound session can be updated");
        }
        let definition = catalog::entity(name)?;
        if definition["set"].is_null() {
            bail!("Use dedicated transitions for leases, messages and deliveries");
        }
        catalog::validate(&definition["set"], input)?;
        if name == "subscriptions" {
            self.call(session, "subscribe", input)?;
            return self.entity_get(session, name, id);
        }
        transaction(&self.db, |db| {
            self.known(session, true)?;
            for field in ["name", "vendorSession"] {
                if let Some(value) = input.get(field) {
                    if !value.is_null() {
                        catalog::text(input, field)?;
                    }
                    if field == "vendorSession" {
                        self.validate_vendor_session_update(session, value)?;
                    }
                    execute(
                        db,
                        &format!("UPDATE sessions SET {field}=? WHERE id=?"),
                        &[value.clone(), json!(id)],
                    )?;
                }
            }
            self.entity_get(session, name, id)
        })
    }
}
fn decode(name: &str, mut row: Value) -> Result<Value> {
    if name == "audit" && !row.is_null() {
        row["data"] = serde_json::from_str(
            row["data"]
                .as_str()
                .ok_or_else(|| anyhow!("Invalid audit data"))?,
        )?;
    }
    if name == "subscriptions" && !row.is_null() {
        row["topics"] = serde_json::from_str(
            row["topics"]
                .as_str()
                .ok_or_else(|| anyhow!("Invalid subscriptions"))?,
        )?;
    }
    Ok(row)
}
