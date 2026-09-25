use crate::{
    database::{execute, query, transaction},
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
        let content = input["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Document content required"))?;
        if content.len() > MAX_BYTES {
            bail!("Document exceeds 1 MiB");
        }
        transaction(&self.db, |db| {
            self.known(session, true)?;
            let directory = directory(&self.workspace, true)?;
            let path = directory.join(name);
            if let Some(record) = self.document_record(name)? {
                if record["sha256"] != digest(content) || read(&path)? != content {
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
            let record = json!({"name":name,"path":format!(".octocode/communication/{name}"),"author":session,"bytes":content.len(),"sha256":digest(content)});
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
