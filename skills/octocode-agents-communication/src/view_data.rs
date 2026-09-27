//! Human operator view: all entities in one workspace, without a synthetic agent.
//! Agent-facing entity visibility remains unchanged.
use crate::{
    database::{query, read_transaction},
    store::{Store, now},
};
use anyhow::{Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

pub const ENTITIES: &[&str] = &[
    "session",
    "message",
    "lease",
    "audit",
    "delivery",
    "dispatch",
    "document",
    "attachment",
    "subscriptions",
];

pub fn summary(store: &Store) -> Result<Value> {
    store.check_database()?;
    read_transaction(&store.db, |db| {
        let counts = query(db, "SELECT
          (SELECT count(*) FROM sessions WHERE workspace=?1 AND expiresAt>?2) AS activeAgents,
          (SELECT count(*) FROM messages m JOIN sessions s ON s.id=m.sender WHERE s.workspace=?1) AS messages,
          (SELECT count(*) FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1 AND l.expiresAt>?2 AND s.expiresAt>?2) AS activeLocks,
          (SELECT count(*) FROM deliveries d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND d.acknowledgedAt IS NULL) AS pending,
          (SELECT count(*) FROM dispatches d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND d.state='uncertain') AS uncertain", &[json!(store.workspace), json!(now())])?;
        Ok(
            json!({"workspace":store.workspace,"database":store.database,"at":now(),"counts":counts.first(),"entities":ENTITIES}),
        )
    })
}

pub fn page(
    store: &Store,
    entity: &str,
    after: Option<&str>,
    agent: Option<&str>,
    filters: &Value,
) -> Result<Value> {
    store.check_database()?;
    let time = now();
    let (sql, agent_field, pair) = match entity {
        "session" => (
            "SELECT s.*,s.expiresAt>?2 AS active FROM sessions s WHERE s.workspace=?1",
            "id",
            false,
        ),
        "message" => (
            "SELECT m.*,m.expiresAt-m.ttlMs AS createdAt,s.name AS senderName,s.vendor AS senderVendor,t.name AS targetName,t.vendor AS targetVendor,(SELECT count(*) FROM deliveries d WHERE d.message=m.id AND d.acknowledgedAt IS NULL) AS pending,(SELECT count(*) FROM deliveries d WHERE d.message=m.id) AS recipients FROM messages m JOIN sessions s ON s.id=m.sender LEFT JOIN sessions t ON t.id=m.target WHERE s.workspace=?1 AND ?2>0",
            "sender",
            false,
        ),
        "lease" => (
            "SELECT l.id,l.workspace,l.path,l.kind,l.owner,s.name AS ownerName,l.reasoning,l.acquiredAt,l.refreshedAt,l.expiresAt FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=?1 AND s.workspace=?1 AND l.expiresAt>?2 AND s.expiresAt>?2",
            "owner",
            false,
        ),
        "audit" => (
            "SELECT a.*,s.name AS agentName,s.vendor FROM audit a JOIN sessions s ON s.id=a.session WHERE s.workspace=?1 AND ?2>0",
            "session",
            false,
        ),
        "delivery" => (
            "SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.*,s.name AS recipientName FROM deliveries d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND ?2>0",
            "recipient",
            true,
        ),
        "dispatch" => (
            "SELECT CAST(d.message AS TEXT)||':'||d.recipient AS id,d.message,d.recipient,s.name AS recipientName,d.transport,d.state,d.attemptedAt,d.submittedAt,d.error FROM dispatches d JOIN sessions s ON s.id=d.recipient WHERE s.workspace=?1 AND ?2>0",
            "recipient",
            true,
        ),
        "document" => (
            "SELECT d.id,d.name,a.session,s.name AS agentName,a.at,a.data FROM documents d JOIN audit a ON a.id=d.id JOIN sessions s ON s.id=a.session WHERE d.workspace=?1 AND s.workspace=?1 AND ?2>0",
            "session",
            false,
        ),
        "attachment" => (
            "SELECT a.session AS id,a.session,a.transport,a.updatedAt,s.name AS agentName FROM attachments a JOIN sessions s ON s.id=a.session WHERE s.workspace=?1 AND ?2>0",
            "session",
            false,
        ),
        "subscriptions" => (
            "SELECT s.id,s.id AS session,s.name AS agentName,coalesce((SELECT json_group_array(topic) FROM subscriptions WHERE session=s.id),'[]') AS topics FROM sessions s WHERE s.workspace=?1 AND ?2>0",
            "session",
            false,
        ),
        _ => bail!("Unknown view entity"),
    };
    let mut args = vec![json!(store.workspace), json!(time)];
    let mut conditions = vec!["1=1".to_owned()];
    if let Some(agent) = agent {
        uuid::Uuid::parse_str(agent)?;
        args.push(json!(agent));
        if entity == "message" {
            let n = args.len();
            conditions.push(format!(
                "(sender=?{n} OR id IN (SELECT message FROM deliveries WHERE recipient=?{n}))"
            ));
        } else {
            conditions.push(format!("{agent_field}=?{}", args.len()));
        }
    }
    if filters.as_object().is_none() {
        bail!("Invalid message filters");
    }
    for (key, value) in filters.as_object().into_iter().flatten() {
        if entity != "message" {
            bail!("Message filters require the Messages view");
        }
        let value = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid filter"))?;
        if value.is_empty() || value.len() > 256 {
            bail!("Filter must contain 1–256 bytes");
        }
        match key.as_str() {
            "q" => {
                args.push(json!(value));
                let n = args.len();
                conditions.push(format!("(instr(lower(body),lower(?{n}))>0 OR instr(lower(senderName),lower(?{n}))>0 OR instr(lower(targetName),lower(?{n}))>0 OR instr(lower(conversationId),lower(?{n}))>0 OR CAST(id AS TEXT)=?{n})"));
            }
            "conversation" => {
                args.push(json!(value));
                conditions.push(format!("conversationId=?{}", args.len()));
            }
            "status" => match value {
                "pending" => conditions.push("pending>0".into()),
                "handled" => conditions.push("pending=0 AND recipients>0".into()),
                _ => bail!("Unknown message status"),
            },
            _ => bail!("Unknown message filter"),
        }
    }
    if let Some(after) = after {
        let cursor: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(after)?)?;
        if cursor["entity"] != entity
            || cursor["agent"] != json!(agent)
            || cursor["filters"] != *filters
        {
            bail!("Cursor belongs to another view; refresh the first page");
        }
        if pair {
            let message = cursor["message"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("Invalid cursor"))?;
            let recipient = cursor["recipient"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid cursor"))?;
            let n = args.len() + 1;
            conditions.push(format!(
                "(message<?{n} OR (message=?{n} AND recipient<?{}))",
                n + 1
            ));
            args.extend([json!(message), json!(recipient)]);
        } else {
            let id = &cursor["id"];
            if !(id.is_string() || id.is_i64()) {
                bail!("Invalid cursor");
            }
            args.push(id.clone());
            conditions.push(format!("id<?{}", args.len()));
        }
    }
    let order = if pair {
        "message DESC,recipient DESC"
    } else {
        "id DESC"
    };
    let mut rows = query(
        &store.db,
        &format!(
            "SELECT * FROM ({sql}) WHERE {} ORDER BY {order} LIMIT 51",
            conditions.join(" AND ")
        ),
        &args,
    )?;
    // Bound responses by both rows and serialized bytes. One large entity still progresses.
    let mut bytes = 0;
    let count = rows
        .iter()
        .take(50)
        .take_while(|row| {
            let size = row.to_string().len();
            if bytes > 0 && bytes + size > 128 * 1024 {
                return false;
            }
            bytes += size;
            true
        })
        .count();
    let next = if rows.len() > count && count > 0 {
        let row = &rows[count - 1];
        Some(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({"entity":entity,"agent":agent,"filters":filters,"id":row["id"],"message":row["message"],"recipient":row["recipient"]}))?))
    } else {
        None
    };
    rows.truncate(count);
    for row in &mut rows {
        for field in ["data", "topics"] {
            if let Some(text) = row[field].as_str() {
                row[field] = serde_json::from_str(text)?;
            }
        }
    }
    Ok(json!({"items":rows,"next":next,"at":time}))
}
