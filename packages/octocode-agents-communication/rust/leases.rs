use crate::{
    catalog::{text, ttl},
    database::{execute, query, transaction},
    paths::{overlap, resolve_path},
    store::{Store, now},
};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::Path;

impl Store {
    pub(crate) fn lock(&self, session: &str, input: &Value, multiple: bool) -> Result<Value> {
        let reasoning = text(input, "reasoning")?;
        let requests = if multiple {
            input["paths"]
                .as_array()
                .ok_or_else(|| anyhow!("Missing paths"))?
                .clone()
        } else {
            vec![input.clone()]
        };
        let mut paths = Vec::with_capacity(requests.len());
        for request in requests {
            let path = resolve_path(Path::new(&self.workspace), text(&request, "path")?)?;
            if !path.starts_with(&self.workspace) {
                bail!("Path escapes workspace");
            }
            let path = path
                .to_str()
                .ok_or_else(|| anyhow!("Path must be UTF-8"))?
                .to_owned();
            let kind = request["kind"].as_str().unwrap_or("file").to_owned();
            if paths
                .iter()
                .any(|(p, k): &(String, String)| overlap(p, k, &path, &kind))
            {
                bail!("Requested paths overlap; use one covering tree lease or distinct paths");
            }
            paths.push((path, kind));
        }
        paths.sort();
        let duration = ttl(input, 60_000)?;
        transaction(&self.db, |db| {
            self.known(session, true)?;
            let at = now();
            let active = query(
                db,
                "SELECT l.*,s.name AS ownerName,s.vendor AS ownerVendor,s.expiresAt AS ownerExpiresAt FROM leases l JOIN sessions s ON s.id=l.owner WHERE l.workspace=? AND l.expiresAt>? AND s.expiresAt>? ORDER BY l.id",
                &[json!(self.workspace), json!(at), json!(at)],
            )?;
            for (path, kind) in &paths {
                for row in &active {
                    if !overlap(
                        row["path"].as_str().unwrap_or(""),
                        row["kind"].as_str().unwrap_or(""),
                        path,
                        kind,
                    ) {
                        continue;
                    }
                    let owner = row["owner"].as_str().unwrap_or("");
                    let expiry = row["expiresAt"]
                        .as_i64()
                        .unwrap_or(at)
                        .min(row["ownerExpiresAt"].as_i64().unwrap_or(at));
                    let held: Vec<Value> = active
                        .iter()
                        .filter(|l| l["owner"] == session)
                        .map(|l| l["id"].clone())
                        .collect();
                    let next = if owner == session {
                        Value::Null
                    } else {
                        json!({"command":"send_message","input":{"to":owner,"body":format!("Need access overlapping your lease {} on {}; can you release it or agree a handoff?",row["id"],row["path"]),"reasoning":reasoning,"key":format!("lease-request-{}-{}",row["id"], &reasoning_key(reasoning)[..16])}})
                    };
                    return Ok(
                        json!({"ok":false,"conflict":{"id":row["id"],"workspace":row["workspace"],"path":row["path"],"kind":row["kind"],"owner":row["owner"],"expiresAt":row["expiresAt"],"reasoning":row["reasoning"]},
                        "owner":{"id":owner,"name":row["ownerName"],"vendor":row["ownerVendor"],"expiresAt":row["ownerExpiresAt"]},
                        "retryAfterMs":(expiry-at).max(0),"heldLeaseIds":held,"next":next,
                        "guidance":if owner == session {"You already hold an overlapping lease. Reuse it if it covers the work, or release it and acquire the complete set."} else {"No leases acquired. Release your held leases before waiting. Ask the owner once, do independent work, then retry after handoff or expiry. Never write on a denied or expired lease; no polling loop."}}),
                    );
                }
            }
            let expires = now() + duration;
            let mut leases = Vec::with_capacity(paths.len());
            for (path, kind) in &paths {
                execute(
                    db,
                    "INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning) VALUES(?,?,?,?,?,?)",
                    &[
                        json!(self.workspace),
                        json!(path),
                        json!(kind),
                        json!(session),
                        json!(expires),
                        json!(reasoning),
                    ],
                )?;
                leases.push(json!({"id":db.last_insert_rowid(),"path":path,"kind":kind,"owner":session,"expiresAt":expires,"reasoning":reasoning}));
            }
            Ok(if multiple {
                json!({"ok":true,"leases":leases})
            } else {
                json!({"ok":true,"lease":leases[0]})
            })
        })
    }
}

fn reasoning_key(reasoning: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(reasoning.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
