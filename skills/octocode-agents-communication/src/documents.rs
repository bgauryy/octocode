use crate::{
    catalog::text,
    database::{execute, query, read_transaction, transaction},
    paths::{overlap, resolve_path},
    store::{PAGE_BYTES, Store, now},
};
use anyhow::{Result, anyhow, bail};
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 1024 * 1024;

fn scope_path(workspace: &str, path: &str) -> Result<String> {
    let resolved = resolve_path(Path::new(workspace), path)?;
    let relative = resolved
        .strip_prefix(workspace)
        .map_err(|_| anyhow!("Context path must stay inside this workspace"))?;
    let relative = relative
        .to_str()
        .ok_or_else(|| anyhow!("Context path must be UTF-8"))?;
    #[cfg(windows)]
    let relative = relative.replace('\\', "/");
    Ok(if relative.is_empty() {
        ".".into()
    } else {
        relative.to_string()
    })
}

fn context_metadata(workspace: &str, input: &Value) -> Result<Value> {
    let Some(context) = input.get("context") else {
        return Ok(Value::Null);
    };
    let mut context = context.clone();
    if context["summary"].as_str().unwrap_or("").trim().is_empty() {
        bail!("Context summary must explain a useful fact or gotcha");
    }
    context["path"] = json!(scope_path(
        workspace,
        context["path"].as_str().unwrap_or(".")
    )?);
    context["kind"] = json!(context["kind"].as_str().unwrap_or("tree"));
    context["ttlMs"] = json!(context["ttlMs"].as_u64().unwrap_or(86_400_000));
    Ok(context)
}

fn name(input: &Value) -> Result<&str> {
    let name = input["name"]
        .as_str()
        .ok_or_else(|| anyhow!("Document name required"))?;
    if name.is_empty()
        || name.len() > 128
        || !name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
        || name.contains("..")
    {
        bail!(
            "Use a lowercase document filename (letters, digits, dot, dash, underscore), without directories or '..'"
        );
    }
    Ok(name)
}

fn directory(workspace: &str, create: bool) -> Result<PathBuf> {
    let mut path = PathBuf::from(workspace);
    for component in [".octocode", "communication"] {
        path.push(component);
        if create {
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        if !fs::symlink_metadata(&path)?.file_type().is_dir() {
            bail!("Document directories must be real directories, not symlinks");
        }
    }
    Ok(path)
}

fn open_document(path: &Path) -> Result<fs::File> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        bail!("Document must be a regular file, not a symlink");
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        bail!("Document must be a regular file");
    }
    Ok(file)
}

fn read(path: &Path) -> Result<String> {
    let mut content = String::new();
    open_document(path)?
        .take((MAX_BYTES + 1) as u64)
        .read_to_string(&mut content)?;
    if content.len() > MAX_BYTES {
        bail!("Document exceeds 1 MiB");
    }
    Ok(content)
}
fn digest(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Documents scanned per context call; a page ends early once `limit` notes match.
const CONTEXT_SCAN: i64 = 200;

impl Store {
    /// Indexed lookup through the workspace document registry (one row per name).
    fn document_record(&self, db: &Connection, name: &str) -> Result<Option<Value>> {
        query(db, "SELECT a.data FROM documents d JOIN audit a ON a.id=d.id WHERE d.workspace=? AND d.name=?", &[json!(self.workspace), json!(name)])?
            .first().map(|row| serde_json::from_str(row["data"].as_str().ok_or_else(|| anyhow!("Invalid document audit"))?).map_err(Into::into)).transpose()
    }
    fn same_document(
        record: &Value,
        path: &Path,
        content: &str,
        metadata: &Value,
        reasoning: &str,
    ) -> Result<Value> {
        let mut stored = record["context"].clone();
        if let Some(object) = stored.as_object_mut() {
            object.remove("expiresAt");
        }
        if record["sha256"] != digest(content)
            || read(path)? != content
            || stored != *metadata
            || record["reasoning"] != reasoning
        {
            bail!("Document is immutable or has changed on disk; publish a new name");
        }
        Ok(json!({"created":false,"document":record}))
    }
    pub(crate) fn share_document(&self, session: &str, input: &Value) -> Result<Value> {
        let name = name(input)?;
        let reasoning = text(input, "reasoning")?;
        let content = input["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Document content required"))?;
        let metadata = context_metadata(&self.workspace, input)?;
        let directory = directory(&self.workspace, true)?;
        let path = directory.join(name);
        // Identical retries finish under a read snapshot, without the writer lock.
        if let Some(record) = read_transaction(&self.db, |db| {
            self.known(session, true)?;
            self.document_record(db, name)
        })? {
            return Self::same_document(&record, &path, content, &metadata, reasoning);
        }
        if fs::symlink_metadata(&path).is_ok() {
            bail!("Unregistered document already exists; preserve it and publish a new name");
        }
        // Write and fsync before taking the writer lock; publication is a rename.
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(content.as_bytes())?;
        temporary.as_file().sync_all()?;
        transaction(&self.db, |db| {
            self.known(session, true)?;
            if let Some(record) = self.document_record(db, name)? {
                return Self::same_document(&record, &path, content, &metadata, reasoning);
            }
            if fs::symlink_metadata(&path).is_ok() {
                bail!("Unregistered document already exists; preserve it and publish a new name");
            }
            // No partially written document is visible and existing names are never overwritten.
            temporary.persist_noclobber(&path)?;
            let mut record = json!({"name":name,"path":format!(".octocode/communication/{name}"),"author":session,"reasoning":reasoning,"bytes":content.len(),"sha256":digest(content)});
            if !metadata.is_null() {
                record["context"] = metadata.clone();
                record["context"]["expiresAt"] =
                    json!(now() + metadata["ttlMs"].as_i64().unwrap_or(86_400_000));
            }
            execute(
                db,
                "INSERT INTO audit(session,kind,entityId,at,data,key) VALUES(?,'document.created',?,?,?,?)",
                &[
                    json!(session),
                    json!(name),
                    json!(now()),
                    json!(record.to_string()),
                    json!(name),
                ],
            )?;
            // A crash before commit can leave an unregistered file; never silently adopt it.
            Ok(json!({"created":true,"document":record}))
        })
    }
    /// Pull scoped summaries, never bodies or host history. Walks this workspace's
    /// document registry in ID order, a bounded number of rows per call.
    pub(crate) fn context(&self, session: &str, input: &Value) -> Result<Value> {
        let path = scope_path(&self.workspace, input["path"].as_str().unwrap_or("."))?;
        let after = input["after"].as_i64().unwrap_or(0);
        let limit = input["limit"].as_u64().unwrap_or(10) as usize;
        read_transaction(&self.db, |db| {
            self.known(session, true)?;
            let through = match input["through"].as_i64() {
                Some(value) => value,
                None => db.query_row(
                    "SELECT coalesce(max(id),0) FROM documents WHERE workspace=?",
                    [&self.workspace],
                    |r| r.get(0),
                )?,
            };
            if through < after {
                bail!("Context through must be at least after");
            }
            let rows = query(
                db,
                "SELECT d.id,a.data FROM documents d JOIN audit a ON a.id=d.id WHERE d.workspace=? AND d.id>? AND d.id<=? ORDER BY d.id LIMIT ?",
                &[
                    json!(self.workspace),
                    json!(after),
                    json!(through),
                    json!(CONTEXT_SCAN),
                ],
            )?;
            let mut items = Vec::new();
            let mut cursor = after;
            let mut scanned = 0;
            let mut bytes = 0;
            let at = now();
            for row in &rows {
                let previous_cursor = cursor;
                cursor = row["id"]
                    .as_i64()
                    .ok_or_else(|| anyhow!("Invalid document ID"))?;
                scanned += 1;
                let record: Value = serde_json::from_str(row["data"].as_str().unwrap_or("{}"))?;
                let context = &record["context"];
                let Some(scope) = context["path"].as_str() else {
                    continue;
                };
                if context["expiresAt"].as_i64().unwrap_or(0) <= at
                    || (!context["branch"].is_null() && context["branch"] != input["branch"])
                    || !overlap(
                        scope,
                        context["kind"].as_str().unwrap_or("tree"),
                        &path,
                        "file",
                    )
                {
                    continue;
                }
                let item = json!({"id":cursor,"name":record["name"],"author":record["author"],"context":context});
                let size = serde_json::to_vec(&item)?.len();
                if !items.is_empty() && bytes + size > PAGE_BYTES {
                    cursor = previous_cursor;
                    scanned -= 1;
                    break;
                }
                bytes += size;
                items.push(item);
                if items.len() == limit {
                    break;
                }
            }
            // Exhausting the registry window is terminal, not a poll loop.
            if scanned == rows.len() && (rows.len() as i64) < CONTEXT_SCAN {
                cursor = through;
            }
            let next = if cursor < through {
                let mut next = input.clone();
                next["after"] = json!(cursor);
                next["through"] = json!(through);
                json!({"command":"context","input":next})
            } else {
                Value::Null
            };
            Ok(json!({"items":items,"cursor":cursor,"scanned":scanned,"next":next}))
        })
    }
    pub(crate) fn read_document(&self, session: &str, input: &Value) -> Result<Value> {
        self.known(session, true)?;
        let name = name(input)?;
        let record = match self.document_record(&self.db, name)? {
            Some(record) => record,
            None => {
                // Do not silently choose a different immutable document. Offer a
                // bounded, workspace-scoped repair for omitted filename extensions.
                let prefix = format!("{name}.");
                let candidates = query(
                    &self.db,
                    "SELECT name FROM documents WHERE workspace=? AND name>=? AND name<? ORDER BY name LIMIT 5",
                    &[
                        json!(self.workspace),
                        json!(prefix),
                        json!(format!("{name}/")),
                    ],
                )?;
                bail!(
                    "Unknown document '{name}' in this workspace. Use the exact published document.name, including its extension. Matching names (up to 5): {}. Read the document successfully before using its contents.",
                    json!(
                        candidates
                            .iter()
                            .map(|row| &row["name"])
                            .collect::<Vec<_>>()
                    )
                );
            }
        };
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        let limit = input["limit"].as_u64().unwrap_or(8192) as usize;
        let bytes = record["bytes"]
            .as_u64()
            .filter(|bytes| *bytes <= MAX_BYTES as u64)
            .ok_or_else(|| anyhow!("Invalid document size"))? as usize;
        if offset > bytes {
            bail!("Offset must be a UTF-8 byte boundary within the document");
        }
        // Reverify the complete file on every call: metadata caches would miss
        // same-size edits outside this page. Retain only this page, not 1 MiB.
        let mut file = open_document(&directory(&self.workspace, false)?.join(name))?
            .take((MAX_BYTES + 1) as u64);
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 8192];
        let mut page = Vec::with_capacity(limit.min(bytes - offset));
        let mut scanned = 0;
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            let from = offset.saturating_sub(scanned).min(count);
            let to = offset
                .saturating_add(limit)
                .saturating_sub(scanned)
                .min(count);
            if from < to {
                page.extend_from_slice(&buffer[from..to]);
            }
            scanned += count;
        }
        let actual: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if scanned != bytes || record["sha256"] != actual {
            bail!("Document integrity mismatch; ask the author to publish a new document");
        }
        let content = match std::str::from_utf8(&page) {
            Ok(content) => content,
            Err(error) if error.error_len().is_none() => {
                std::str::from_utf8(&page[..error.valid_up_to()])?
            }
            Err(_) => bail!("Offset must be a UTF-8 byte boundary within the document"),
        };
        let end = offset + content.len();
        Ok(
            json!({"document":record,"offset":offset,"content":content,"next":if end < bytes { json!({"command":"read_document","input":{"name":name,"offset":end,"limit":limit}}) } else { Value::Null }}),
        )
    }
}
