use crate::{
    catalog::text,
    database::{execute, query, transaction},
    paths::{overlap, resolve_path},
    store::{Store, now},
};
use anyhow::{Result, anyhow, bail};
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

fn read(path: &Path) -> Result<String> {
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
    let mut content = String::new();
    file.take((MAX_BYTES + 1) as u64)
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

impl Store {
    fn document_record(&self, name: &str) -> Result<Option<Value>> {
        query(&self.db, "SELECT a.data FROM audit a JOIN sessions s ON s.id=a.session WHERE s.workspace=? AND a.kind='document.created' AND a.key=? ORDER BY a.id LIMIT 1", &[json!(self.workspace), json!(name)])?
            .first().map(|row| serde_json::from_str(row["data"].as_str().ok_or_else(|| anyhow!("Invalid document audit"))?).map_err(Into::into)).transpose()
    }
    pub(crate) fn share_document(&self, session: &str, input: &Value) -> Result<Value> {
        let name = name(input)?;
        let reasoning = text(input, "reasoning")?;
        let content = input["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Document content required"))?;
        if content.len() > MAX_BYTES {
            bail!("Document exceeds 1 MiB");
        }
        let metadata = context_metadata(&self.workspace, input)?;
        transaction(&self.db, |db| {
            self.known(session, true)?;
            let directory = directory(&self.workspace, true)?;
            let path = directory.join(name);
            if let Some(record) = self.document_record(name)? {
                let mut stored = record["context"].clone();
                if let Some(object) = stored.as_object_mut() {
                    object.remove("expiresAt");
                }
                if record["sha256"] != digest(content)
                    || read(&path)? != content
                    || stored != metadata
                    || record["reasoning"] != reasoning
                {
                    bail!("Document is immutable or has changed on disk; publish a new name");
                }
                return Ok(json!({"created":false,"document":record}));
            }
            if fs::symlink_metadata(&path).is_ok() {
                bail!("Unregistered document already exists; preserve it and publish a new name");
            }
            let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
            temporary.write_all(content.as_bytes())?;
            temporary.as_file().sync_all()?;
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
    /// Pull scoped summaries, never bodies or host history. Scan a bounded audit
    /// window so sparse matches also return a resumable continuation.
    pub(crate) fn context(&self, session: &str, input: &Value) -> Result<Value> {
        self.known(session, true)?;
        let path = scope_path(&self.workspace, input["path"].as_str().unwrap_or("."))?;
        let after = input["after"].as_i64().unwrap_or(0);
        let through = match input["through"].as_i64() {
            Some(value) => value,
            None => query(&self.db, "SELECT coalesce(max(id),0) AS id FROM audit", &[])?[0]["id"]
                .as_i64()
                .unwrap_or(0),
        };
        if through < after {
            bail!("Context through must be at least after");
        }
        let limit = input["limit"].as_u64().unwrap_or(10) as usize;
        let rows = query(
            &self.db,
            "SELECT a.id,CASE WHEN a.kind='document.created' AND s.workspace=? THEN a.data END AS data FROM (SELECT id,session,kind,data FROM audit WHERE id>? AND id<=? ORDER BY id LIMIT 200) a LEFT JOIN sessions s ON s.id=a.session ORDER BY a.id",
            &[json!(self.workspace), json!(after), json!(through)],
        )?;
        let mut items = Vec::new();
        let mut cursor = after;
        let mut scanned = 0;
        let at = now();
        for row in &rows {
            cursor = row["id"]
                .as_i64()
                .ok_or_else(|| anyhow!("Invalid audit ID"))?;
            scanned += 1;
            let Some(data) = row["data"].as_str() else {
                continue;
            };
            let record: Value = serde_json::from_str(data)?;
            let context = &record["context"];
            let Some(scope) = context["path"].as_str() else {
                continue;
            };
            if context["expiresAt"].as_i64().unwrap_or(0) <= at
                || (!context["branch"].is_null() && context["branch"] != input["branch"])
                || !(scope == "." && context["kind"] == "tree"
                    || overlap(
                        scope,
                        context["kind"].as_str().unwrap_or("tree"),
                        &path,
                        "file",
                    ))
            {
                continue;
            }
            items.push(json!({"id":cursor,"name":record["name"],"author":record["author"],"context":context}));
            if items.len() == limit {
                break;
            }
        }
        // Gaps (or a cursor beyond current history) are terminal, not a poll loop.
        if scanned == rows.len() && rows.len() < 200 {
            cursor = through;
        }
        let next = if cursor < through {
            let mut next = input.clone();
            next["after"] = json!(cursor);
            next["through"] = json!(through);
            Some(next)
        } else {
            None
        };
        Ok(json!({"items":items,"cursor":cursor,"scanned":scanned,"next":next}))
    }
    pub(crate) fn read_document(&self, session: &str, input: &Value) -> Result<Value> {
        self.known(session, true)?;
        let name = name(input)?;
        let record = match self.document_record(name)? {
            Some(record) => record,
            None => {
                // Do not silently choose a different immutable document. Offer a
                // bounded, workspace-scoped repair for omitted filename extensions.
                let prefix = format!("{name}.");
                let candidates = query(
                    &self.db,
                    "SELECT DISTINCT a.key AS name FROM audit a JOIN sessions s ON s.id=a.session WHERE s.workspace=? AND a.kind='document.created' AND substr(a.key,1,length(?))=? ORDER BY a.key LIMIT 5",
                    &[json!(self.workspace), json!(prefix), json!(prefix)],
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
        let content = read(&directory(&self.workspace, false)?.join(name))?;
        if record["sha256"] != digest(&content)
            || record["bytes"].as_u64() != Some(content.len() as u64)
        {
            bail!("Document integrity mismatch; ask the author to publish a new document");
        }
        let offset = input["offset"].as_u64().unwrap_or(0) as usize;
        let limit = input["limit"].as_u64().unwrap_or(8192) as usize;
        if !content.is_char_boundary(offset) {
            bail!("Offset must be a UTF-8 byte boundary within the document");
        }
        let mut end = offset.saturating_add(limit).min(content.len());
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        Ok(
            json!({"document":record,"offset":offset,"content":&content[offset..end],"next":if end < content.len() { json!({"name":name,"offset":end,"limit":limit}) } else { Value::Null }}),
        )
    }
}
