use crate::{
    catalog,
    database::{execute, query, transaction},
    store::{Store, now, page, strip_nulls},
};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

/// Cursor order of an entity: integer IDs, text IDs, or `<message>:<recipient>` pairs.
#[derive(Clone, Copy, PartialEq)]
enum Order {
    Integer,
    Text,
    Pair,
}

impl Store {
    fn entity_query(&self, session: &str, name: &str) -> Result<(String, Vec<Value>, Order)> {
        self.known(session, false)?;
        let visible_messages = "SELECT id FROM messages WHERE sender=?1";
        let (sql, args, order) = match name {
            "session" => (
                "SELECT *,expiresAt>?2 AS active FROM sessions WHERE workspace=?1".to_owned(),
                vec![json!(self.workspace), json!(now())],
                Order::Text,
            ),
            "lease" => (
                "SELECT l.*,l.expiresAt>?2 AND s.expiresAt>?2 AS active FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1".to_owned(),
                vec![json!(self.workspace), json!(now())],
                Order::Integer,
            ),
            // Start from this session's own sent/received index entries, not all messages.
            "message" => (
                format!("SELECT m.* FROM messages m WHERE m.id IN ({visible_messages} UNION SELECT message FROM deliveries WHERE recipient=?1) AND EXISTS(SELECT 1 FROM sessions s WHERE s.id=m.sender AND s.workspace=?2)"),
                vec![json!(session), json!(self.workspace)],
                Order::Integer,
            ),
            "delivery" => (
                format!("SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.* FROM deliveries d WHERE (d.recipient=?1 OR d.message IN ({visible_messages})) AND EXISTS(SELECT 1 FROM messages m JOIN sessions s ON s.id=m.sender WHERE m.id=d.message AND s.workspace=?2)"),
                vec![json!(session), json!(self.workspace)],
                Order::Pair,
            ),
            "subscriptions" => (
                "SELECT s.id,s.id AS session,coalesce((SELECT json_group_array(topic) FROM (SELECT topic FROM subscriptions WHERE session=s.id ORDER BY topic)), '[]') AS topics FROM sessions s WHERE s.workspace=?1".to_owned(),
                vec![json!(self.workspace)],
                Order::Text,
            ),
            "attachment" => (
                "SELECT a.session AS id,a.* FROM attachments a JOIN sessions s ON s.id=a.session WHERE s.workspace=?1".to_owned(),
                vec![json!(self.workspace)],
                Order::Text,
            ),
            "dispatch" => (
                format!("SELECT CAST(x.message AS TEXT)||':'||x.recipient AS id,x.* FROM dispatches x WHERE (x.recipient=?1 OR x.message IN ({visible_messages})) AND EXISTS(SELECT 1 FROM messages m JOIN sessions s ON s.id=m.sender WHERE m.id=x.message AND s.workspace=?2)"),
                vec![json!(session), json!(self.workspace)],
                Order::Pair,
            ),
            // Walk audit in ID order so a page stops after its rows instead of sorting history.
            "audit" => (
                "SELECT a.* FROM audit a WHERE EXISTS(SELECT 1 FROM sessions s WHERE s.id=a.session AND s.workspace=?1)".to_owned(),
                vec![json!(self.workspace)],
                Order::Integer,
            ),
            _ => bail!("Unknown entity: {name}"),
        };
        Ok((sql, args, order))
    }
    pub fn entity_get(&self, session: &str, name: &str, id: &str) -> Result<Value> {
        let (sql, mut args, _) = self.entity_query(session, name)?;
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
        let (sql, mut args, order) = self.entity_query(session, name)?;
        let mut conditions = Vec::<String>::new();
        if let Some(after) = filters["after"].as_str() {
            match order {
                Order::Text => {
                    conditions.push("id>?".into());
                    args.push(json!(after));
                }
                Order::Integer => {
                    conditions.push("id>?".into());
                    args.push(json!(
                        after
                            .parse::<i64>()
                            .map_err(|_| anyhow!("after must be a previous next value"))?
                    ));
                }
                Order::Pair => {
                    let (message, recipient) = after
                        .split_once(':')
                        .and_then(|(m, r)| Some((m.parse::<i64>().ok()?, r)))
                        .ok_or_else(|| anyhow!("after must be a previous next value"))?;
                    conditions.push("(message>? OR (message=? AND recipient>?))".into());
                    args.extend([json!(message), json!(message), json!(recipient)]);
                }
            }
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
        if filters.get("path").is_some() {
            let target = self.lease_target(filters)?;
            conditions.push("(pathKey=? OR (kind='tree' AND pathKey IN (SELECT value FROM json_each(?))) OR (?='tree' AND pathKey>? AND pathKey<?))".into());
            args.extend(target.key_args());
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
        let order = if order == Order::Pair {
            "message,recipient"
        } else {
            "id"
        };
        let rows = query(
            &self.db,
            &format!("SELECT * FROM ({sql}) e {clauses} ORDER BY {order} LIMIT 101"),
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
            let identity = self.known(session, true)?;
            for field in ["name", "vendorSession"] {
                if let Some(value) = input.get(field) {
                    if !value.is_null() {
                        catalog::text(input, field)?;
                    }
                    if field == "vendorSession" {
                        self.validate_vendor_session_update(session, &identity, value)?;
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
    if let Some(object) = row.as_object_mut() {
        // The folded lease key is an index column, not part of the entity.
        object.remove("pathKey");
    }
    if name == "audit" && !row.is_null() {
        row["data"] = serde_json::from_str(
            row["data"]
                .as_str()
                .ok_or_else(|| anyhow!("Invalid audit data"))?,
        )?;
        // History written before v7 may carry null members.
        strip_nulls(&mut row["data"]);
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
