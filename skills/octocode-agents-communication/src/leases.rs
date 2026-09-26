use crate::{
    catalog::{text, ttl},
    database::{execute, query, read_transaction, transaction},
    paths::{ancestors, keys_overlap, lease_key, resolve_path},
    store::{Store, now},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path};

/// Most conflicts one check_paths call reports; `truncated` marks a larger set.
const CHECK_LIMIT: i64 = 100;

/// Live leases overlapping one request, via indexed key lookups: the exact key,
/// tree leases on each ancestor key, and (for a tree request) the descendant range.
const OVERLAPS: &str = "WITH hits(id) AS (
  SELECT id FROM leases WHERE workspace=?1 AND pathKey=?2
  UNION ALL SELECT id FROM leases WHERE workspace=?1 AND kind='tree' AND pathKey IN (SELECT value FROM json_each(?3))
  UNION ALL SELECT id FROM leases WHERE ?4='tree' AND workspace=?1 AND pathKey>?5 AND pathKey<?6)
SELECT l.id,l.path,l.kind,l.owner,l.expiresAt,l.reasoning,s.name AS ownerName,s.vendor AS ownerVendor,s.expiresAt AS ownerExpiresAt
FROM hits h JOIN leases l ON l.id=h.id JOIN sessions s ON s.id=l.owner
WHERE l.expiresAt>?7 AND s.expiresAt>?7 AND l.owner<>?8 ORDER BY l.id LIMIT ?9";

pub(crate) struct Target {
    pub path: String,
    pub kind: String,
    pub key: String,
}

impl Target {
    /// SQL arguments ?2..?6 of the key-overlap predicate.
    pub fn key_args(&self) -> [Value; 5] {
        [
            json!(self.key),
            json!(json!(ancestors(&self.key)).to_string()),
            json!(self.kind),
            json!(format!("{}/", self.key)),
            json!(format!("{}0", self.key)),
        ]
    }
}

impl Store {
    /// Resolve one `{path,kind?}` request inside the workspace and fold its key once.
    pub(crate) fn lease_target(&self, request: &Value) -> Result<Target> {
        let path = resolve_path(Path::new(&self.workspace), text(request, "path")?)?;
        if !path.starts_with(&self.workspace) {
            bail!("Path escapes workspace");
        }
        let path = path
            .to_str()
            .ok_or_else(|| anyhow!("Path must be UTF-8"))?
            .to_owned();
        Ok(Target {
            key: lease_key(&path),
            kind: request["kind"].as_str().unwrap_or("file").to_owned(),
            path,
        })
    }
    /// Workspace-relative display path; the workspace itself is ".".
    pub(crate) fn relative(&self, path: &str) -> String {
        match Path::new(path).strip_prefix(&self.workspace) {
            Ok(rest) if rest.as_os_str().is_empty() => ".".into(),
            Ok(rest) => rest.to_string_lossy().replace('\\', "/"),
            Err(_) => path.into(),
        }
    }
    fn overlapping(
        &self,
        db: &Connection,
        target: &Target,
        at: i64,
        exclude_owner: &str,
        limit: i64,
    ) -> Result<Vec<Value>> {
        let mut args = vec![json!(self.workspace)];
        args.extend(target.key_args());
        args.extend([json!(at), json!(exclude_owner), json!(limit)]);
        query(db, OVERLAPS, &args)
    }
    /// Foreign live conflicts for explicit mutations; one read snapshot, no writes.
    pub(crate) fn check_paths(&self, session: &str, input: &Value) -> Result<Value> {
        let targets = input["paths"]
            .as_array()
            .ok_or_else(|| anyhow!("paths required"))?
            .iter()
            .map(|request| self.lease_target(request))
            .collect::<Result<Vec<_>>>()?;
        read_transaction(&self.db, |db| {
            self.known(session, true)?;
            let at = now();
            let mut seen = HashSet::new();
            let mut conflicts = Vec::new();
            let mut truncated = false;
            for target in &targets {
                for lease in self.overlapping(db, target, at, session, CHECK_LIMIT + 1)? {
                    if !seen.insert(lease["id"].as_i64().unwrap_or(0)) {
                        continue;
                    }
                    if conflicts.len() as i64 == CHECK_LIMIT {
                        truncated = true;
                        break;
                    }
                    conflicts.push(json!({"id":lease["id"],"path":self.relative(lease["path"].as_str().unwrap_or("")),
                        "kind":lease["kind"],"owner":lease["owner"],"expiresAt":lease["expiresAt"],"reasoning":lease["reasoning"]}));
                }
                if truncated {
                    break;
                }
            }
            let mut result = json!({"ok":conflicts.is_empty(),"conflicts":conflicts});
            if truncated {
                result["truncated"] = json!(true);
            }
            Ok(result)
        })
    }
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
        let mut targets: Vec<Target> = Vec::with_capacity(requests.len());
        for request in &requests {
            let target = self.lease_target(request)?;
            if targets
                .iter()
                .any(|t| keys_overlap(&t.key, &t.kind, &target.key, &target.kind))
            {
                bail!("Requested paths overlap; use one covering tree lease or distinct paths");
            }
            targets.push(target);
        }
        targets.sort_by(|a, b| (&a.path, &a.kind).cmp(&(&b.path, &b.kind)));
        let duration = ttl(input, 60_000)?;
        transaction(&self.db, |db| {
            self.known(session, true)?;
            let at = now();
            for target in &targets {
                let Some(row) = self.overlapping(db, target, at, "", 1)?.into_iter().next() else {
                    continue;
                };
                let owner = row["owner"].as_str().unwrap_or("");
                let expiry = row["expiresAt"]
                    .as_i64()
                    .unwrap_or(at)
                    .min(row["ownerExpiresAt"].as_i64().unwrap_or(at));
                let held: Vec<Value> = query(
                    db,
                    "SELECT id FROM leases WHERE owner=? AND workspace=? AND expiresAt>? ORDER BY id",
                    &[json!(session), json!(self.workspace), json!(at)],
                )?
                .into_iter()
                .map(|l| l["id"].clone())
                .collect();
                let path = self.relative(row["path"].as_str().unwrap_or(""));
                let mut result = json!({"ok":false,
                    "conflict":{"id":row["id"],"path":path,"kind":row["kind"],"expiresAt":row["expiresAt"],"reasoning":row["reasoning"]},
                    "owner":{"id":owner,"name":row["ownerName"],"vendor":row["ownerVendor"],"expiresAt":row["ownerExpiresAt"]},
                    "retryAfterMs":(expiry-at).max(0),"heldLeaseIds":held});
                if owner == session {
                    result["guidance"] = json!(
                        "You hold an overlapping lease: reuse it, or unlock it and lock the complete set."
                    );
                } else {
                    result["next"] = json!({"command":"send_message","input":{"to":owner,"body":format!("Need access overlapping your lease {} on {path}; can you release it or agree a handoff?",row["id"]),"reasoning":reasoning,"key":format!("lease-request-{}-{}",row["id"], &reasoning_key(reasoning)[..16])}});
                    result["guidance"] = json!(
                        "Nothing acquired. Unlock held leases, send next once, do independent work, retry after handoff or expiry. Never write without ok:true; do not poll."
                    );
                }
                return Ok(result);
            }
            let expires = now() + duration;
            let mut leases = Vec::with_capacity(targets.len());
            for target in &targets {
                execute(
                    db,
                    "INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?)",
                    &[
                        json!(self.workspace),
                        json!(target.path),
                        json!(target.kind),
                        json!(session),
                        json!(expires),
                        json!(reasoning),
                        json!(target.key),
                    ],
                )?;
                leases.push(json!({"id":db.last_insert_rowid(),"path":self.relative(&target.path),"kind":target.kind}));
            }
            Ok(if multiple {
                json!({"ok":true,"expiresAt":expires,"leases":leases})
            } else {
                let mut lease = leases.swap_remove(0);
                lease["expiresAt"] = json!(expires);
                json!({"ok":true,"lease":lease})
            })
        })
    }
    /// Renew or release one owned live lease by acquisition ID.
    pub(crate) fn lease_transition(
        &self,
        session: &str,
        input: &Value,
        renew: bool,
    ) -> Result<Value> {
        let lease = input["leaseId"].clone();
        transaction(&self.db, |db| {
            self.known(session, true)?;
            let at = now();
            Ok(if renew {
                let expires = at + ttl(input, 60_000)?;
                let count = execute(
                    db,
                    "UPDATE leases SET expiresAt=? WHERE id=? AND owner=? AND expiresAt>?",
                    &[json!(expires), lease, json!(session), json!(at)],
                )?;
                if count == 1 {
                    json!({"renewed":true,"expiresAt":expires})
                } else {
                    json!({"renewed":false})
                }
            } else {
                let count = execute(
                    db,
                    "DELETE FROM leases WHERE id=? AND owner=? AND expiresAt>?",
                    &[lease, json!(session), json!(at)],
                )?;
                json!({"released":count==1})
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
