//! Read-only admission check for declared file targets, not an OS write fence.
use crate::{
    catalog,
    database::query,
    paths::resolve_path,
    store::{Store, now},
};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::Path;

impl Store {
    /// Input is validated by `Store::call`, its only caller.
    pub(crate) fn check_write(&self, session: &str, input: &Value) -> Result<Value> {
        let paths = input["paths"]
            .as_array()
            .ok_or_else(|| anyhow!("paths required"))?;
        let mut targets = Vec::with_capacity(paths.len());
        for path in paths {
            let target = resolve_path(
                Path::new(&self.workspace),
                path.as_str().ok_or_else(|| anyhow!("File path required"))?,
            )?;
            if !target.starts_with(&self.workspace) {
                bail!("Path escapes workspace");
            }
            if target.is_dir() {
                bail!("check_write accepts file targets only, not directories");
            }
            targets.push(
                target
                    .to_str()
                    .ok_or_else(|| anyhow!("Path must be UTF-8"))?
                    .to_owned(),
            );
        }
        // One read snapshot: no writer acquisition, implicit renewal, or ownership changes.
        let transaction = self.db.unchecked_transaction()?;
        let identity = self.known(session, true)?;
        if input.get("vendorSession").is_some() {
            catalog::text(input, "vendorSession")?;
            if identity["vendorSession"] != input["vendorSession"] {
                bail!("Native session does not match the bound DB identity");
            }
        }
        let at = now();
        let presence = identity["expiresAt"]
            .as_i64()
            .ok_or_else(|| anyhow!("Invalid presence deadline"))?;
        if presence <= at {
            bail!("Session expired before checking file coverage");
        }
        let mut checks = Vec::with_capacity(targets.len());
        for path in targets {
            let lease = query(&transaction,
                "SELECT id,expiresAt FROM leases WHERE owner=? AND workspace=? AND expiresAt>? AND lease_overlap(path,kind,?,'file') ORDER BY id LIMIT 1",
                &[json!(session), json!(self.workspace), json!(at), json!(path)])?.into_iter().next();
            let mut check = json!({"path":path,"covered":lease.is_some()});
            if let Some(lease) = lease {
                let expires = lease["expiresAt"]
                    .as_i64()
                    .ok_or_else(|| anyhow!("Invalid lease deadline"))?;
                check["lease"] = json!({"id":lease["id"],"expiresAt":expires.min(presence)});
            }
            checks.push(check);
        }
        let ok = checks.iter().all(|check| check["covered"] == true);
        transaction.commit()?;
        Ok(json!({"ok":ok,"checkedAt":at,"advisory":true,"checks":checks}))
    }
}
