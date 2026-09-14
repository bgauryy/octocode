use super::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
    RequestSpec,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const META_FILE: &str = ".octocode-clone-meta.json";
const STALE_LOCK_AGE: Duration = Duration::from_secs(5 * 60);
static STAGE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentsEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ContentsListing {
    pub entries: Vec<ContentsEntry>,
    pub raw_entry_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TreeCacheMeta {
    cloned_at: String,
    expires_at: String,
    owner: String,
    repo: String,
    branch: String,
    commit_sha: String,
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snapshot_id: Option<String>,
}

impl<R: CredentialResolver> GitHubTransport<R> {
    pub async fn repository_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<ContentsListing, ProviderError> {
        let mut segments = vec!["repos", owner, repo, "contents"];
        if !path.is_empty() && path != "." {
            segments.push(path);
        }
        let mut url = self.endpoint().rest(&segments)?;
        url.query_pairs_mut().append_pair("ref", reference);
        let response = self.execute(RequestSpec::get(url), context).await?;
        let value: serde_json::Value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub repository contents response",
            )
        })?;
        let raw_entry_count = value.as_array().map_or(1, Vec::len);
        let raw_entries = match value {
            serde_json::Value::Array(entries) => entries,
            entry @ serde_json::Value::Object(_) => vec![entry],
            _ => {
                return Err(ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub repository contents response",
                ));
            }
        };
        let entries = raw_entries
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub repository contents entry",
                )
            })?;
        Ok(ContentsListing {
            entries,
            raw_entry_count,
        })
    }

    pub async fn resolve_commit_sha(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        if reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok(reference.to_ascii_lowercase());
        }
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "commits", reference])?;
        let response = self.execute(RequestSpec::get(url), context).await?;
        let payload: CommitShaPayload = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub commit response")
        })?;
        if payload.sha.len() != 40 || !payload.sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ProviderError::new(
                ProviderErrorKind::Decode,
                "GitHub returned an invalid commit SHA",
            ));
        }
        Ok(payload.sha.to_ascii_lowercase())
    }

    pub async fn get_blob(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        context: &RequestContext,
    ) -> Result<Vec<u8>, ProviderError> {
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "git", "blobs", sha])?;
        let response = self.execute(RequestSpec::get(url), context).await?;
        let payload: BlobPayload = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub blob response")
        })?;
        decode_blob(payload.encoding.as_deref(), payload.content)
    }
}

#[derive(Deserialize)]
struct CommitShaPayload {
    sha: String,
}

#[derive(Deserialize)]
struct BlobPayload {
    encoding: Option<String>,
    content: Option<String>,
}

fn decode_blob(encoding: Option<&str>, content: Option<String>) -> Result<Vec<u8>, ProviderError> {
    let bytes = match encoding {
        Some("base64") => STANDARD
            .decode(content.unwrap_or_default().replace(['\r', '\n'], ""))
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid base64 file content")
            })?,
        Some("utf-8") => content.unwrap_or_default().into_bytes(),
        _ => {
            return Err(ProviderError::new(
                ProviderErrorKind::Decode,
                "unsupported GitHub content encoding",
            ));
        }
    };
    if bytes.contains(&0) {
        return Err(ProviderError::new(
            ProviderErrorKind::Decode,
            "binary files are not supported",
        ));
    }
    Ok(bytes)
}

pub fn tree_cache_root(home: &Path, owner: &str, repo: &str, commit_sha: &str) -> PathBuf {
    home.join("tmp")
        .join("tree")
        .join(owner)
        .join(repo)
        .join(commit_sha.to_ascii_lowercase())
}

pub fn current_tree_snapshot(cache_root: &Path) -> Option<PathBuf> {
    let bytes = fs::read(cache_root.join(META_FILE)).ok()?;
    let meta: TreeCacheMeta = serde_json::from_slice(&bytes).ok()?;
    let id = meta.snapshot_id.filter(|value| is_snapshot_id(value))?;
    let root = cache_root.join("snapshots").join(id);
    root.is_dir().then_some(root)
}

pub fn publish_tree_snapshot(
    home: &Path,
    cache_root: &Path,
    owner: &str,
    repo: &str,
    branch: &str,
    commit_sha: &str,
    write: impl FnOnce(&Path) -> Result<(), ProviderError>,
) -> Result<PathBuf, ProviderError> {
    let _lock = TreeLock::acquire(home, cache_root)?;
    let previous = current_tree_snapshot(cache_root);
    let staging_base = home.join("tmp").join("tree-staging");
    create_private_dir(&staging_base)?;
    let stage = staging_base.join(format!(
        "snapshot-{}-{}-{}",
        std::process::id(),
        now_millis(),
        STAGE_ID.fetch_add(1, Ordering::Relaxed)
    ));
    remove_dir(&stage);
    create_private_dir(&stage)?;
    let id = snapshot_id();
    let destination = cache_root.join("snapshots").join(&id);
    let pointer = cache_root.join(format!(".snapshot-{id}.tmp"));
    let mut published = false;
    let result = (|| {
        if let Some(previous) = previous.as_ref() {
            copy_tree(previous, &stage)?;
        }
        write(&stage)?;
        if let Some(parent) = destination.parent() {
            create_private_dir(parent)?;
        }
        fs::rename(&stage, &destination).map_err(tree_io)?;
        let meta = TreeCacheMeta {
            cloned_at: iso_millis(now_millis()),
            expires_at: iso_millis(now_millis().saturating_add(24 * 60 * 60 * 1000)),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            branch: branch.to_owned(),
            commit_sha: commit_sha.to_owned(),
            source: "treeFetch".into(),
            snapshot_id: Some(id.clone()),
        };
        let bytes = serde_json::to_vec(&meta)
            .map_err(|error| ProviderError::new(ProviderErrorKind::Decode, error.to_string()))?;
        write_private(&pointer, &bytes).map_err(tree_io)?;
        fs::rename(&pointer, cache_root.join(META_FILE)).map_err(tree_io)?;
        published = true;
        Ok(destination.clone())
    })();
    let _ = fs::remove_dir_all(&stage);
    let _ = fs::remove_file(&pointer);
    if !published {
        let _ = fs::remove_dir_all(&destination);
    }
    result
}

pub fn safe_snapshot_path(root: &Path, relative: &str) -> Result<PathBuf, ProviderError> {
    let mut out = root.to_path_buf();
    for part in relative.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".."
            || part == META_FILE
            || part == ".octocode-directory-meta.json"
            || part.contains('\\')
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                format!(
                    "Path \"{relative}\" escapes the repository directory. Path traversal is not allowed."
                ),
            ));
        }
        out.push(part);
        if out
            .symlink_metadata()
            .ok()
            .is_some_and(|meta| meta.file_type().is_symlink())
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                "Snapshot paths cannot traverse symbolic links.",
            ));
        }
    }
    if !out.starts_with(root) {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "Path \"{relative}\" escapes the repository directory. Path traversal is not allowed."
            ),
        ));
    }
    Ok(out)
}

pub fn ensure_snapshot_directory(root: &Path, relative: &str) -> Result<PathBuf, ProviderError> {
    let directory = safe_snapshot_path(root, relative)?;
    create_private_dir(&directory)?;
    Ok(directory)
}

pub fn write_snapshot_file(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), ProviderError> {
    let path = safe_snapshot_path(root, relative)?;
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    write_private(&path, bytes).map_err(tree_io)
}

fn is_snapshot_id(value: &str) -> bool {
    value.len() == 36
        && value.as_bytes().iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn snapshot_id() -> String {
    let mut digest = Sha256::new();
    digest.update(std::process::id().to_le_bytes());
    digest.update(STAGE_ID.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    if let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) {
        digest.update(duration.as_nanos().to_le_bytes());
    }
    let hex = hex::encode(digest.finalize());
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

struct TreeLock {
    path: PathBuf,
}

impl TreeLock {
    fn acquire(home: &Path, cache_root: &Path) -> Result<Self, ProviderError> {
        let mut digest = Sha256::new();
        digest.update(cache_root.to_string_lossy().as_bytes());
        let path = home
            .join("tmp")
            .join("tree-locks")
            .join(hex::encode(digest.finalize()));
        if let Some(parent) = path.parent() {
            create_private_dir(parent)?;
        }
        let started = Instant::now();
        loop {
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if recover_stale_lock(&path) {
                        continue;
                    }
                    if started.elapsed() >= Duration::from_secs(30) {
                        return Err(ProviderError::new(
                            ProviderErrorKind::Timeout,
                            "Timed out waiting for tree materialization lock.",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(tree_io(error)),
            }
        }
    }
}

impl Drop for TreeLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn recover_stale_lock(path: &Path) -> bool {
    let stale = fs::metadata(path)
        .and_then(|value| value.modified())
        .ok()
        .and_then(|value| SystemTime::now().duration_since(value).ok())
        .is_some_and(|age| age > STALE_LOCK_AGE);
    if !stale {
        return false;
    }
    fs::remove_dir_all(path).is_ok()
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), ProviderError> {
    create_private_dir(destination)?;
    let entries = fs::read_dir(source).map_err(tree_io)?;
    for entry in entries {
        let entry = entry.map_err(tree_io)?;
        let meta = fs::symlink_metadata(entry.path()).map_err(tree_io)?;
        let to = destination.join(entry.file_name());
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else if meta.is_file() {
            fs::copy(entry.path(), &to).map_err(tree_io)?;
        }
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<(), ProviderError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(tree_io)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path).map_err(tree_io)
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    fs::write(path, bytes)
}

fn remove_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn tree_io(error: io::Error) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Transport,
        format!("Tree cache operation failed: {error}"),
    )
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn iso_millis(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let remainder = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        remainder / 3_600_000,
        remainder / 60_000 % 60,
        remainder / 1_000 % 60,
        remainder % 1_000
    )
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "octocode-tree-{}-{}-{nonce}",
            std::process::id(),
            STAGE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("temp home");
        path
    }

    #[test]
    fn snapshot_id_matches_uuid_shape() {
        assert!(is_snapshot_id(&snapshot_id()));
    }

    #[test]
    fn safe_snapshot_path_rejects_escape() {
        let root = PathBuf::from("/tmp/tree-root");
        assert!(safe_snapshot_path(&root, "../secret").is_err());
        assert!(safe_snapshot_path(&root, "ok/file.rs").is_ok());
    }

    #[test]
    fn publish_copies_previous_generation() {
        let home = temp_home();
        let cache = tree_cache_root(&home, "o", "r", "0123456789abcdef0123456789abcdef01234567");
        let first = publish_tree_snapshot(
            &home,
            &cache,
            "o",
            "r",
            "main",
            "0123456789abcdef0123456789abcdef01234567",
            |root| {
                write_snapshot_file(root, "a.rs", b"one")?;
                Ok(())
            },
        )
        .expect("first generation");
        let second = publish_tree_snapshot(
            &home,
            &cache,
            "o",
            "r",
            "main",
            "0123456789abcdef0123456789abcdef01234567",
            |root| {
                write_snapshot_file(root, "b.rs", b"two")?;
                Ok(())
            },
        )
        .expect("second generation");
        assert_ne!(first, second);
        assert_eq!(fs::read(first.join("a.rs")).expect("first a"), b"one");
        assert_eq!(fs::read(second.join("a.rs")).expect("copied a"), b"one");
        assert_eq!(fs::read(second.join("b.rs")).expect("new b"), b"two");
        assert!(!first.join("b.rs").exists());
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn failed_publish_keeps_previous_generation() {
        let home = temp_home();
        let cache = tree_cache_root(&home, "o", "r", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let first = publish_tree_snapshot(
            &home,
            &cache,
            "o",
            "r",
            "main",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            |root| {
                write_snapshot_file(root, "keep.rs", b"keep")?;
                Ok(())
            },
        )
        .expect("first");
        let error = publish_tree_snapshot(
            &home,
            &cache,
            "o",
            "r",
            "main",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            |_| {
                Err(ProviderError::new(
                    ProviderErrorKind::Validation,
                    "forced failure",
                ))
            },
        )
        .expect_err("failed publish");
        assert_eq!(error.message.as_ref(), "forced failure");
        assert_eq!(current_tree_snapshot(&cache), Some(first.clone()));
        assert_eq!(fs::read(first.join("keep.rs")).expect("kept"), b"keep");
        let _ = fs::remove_dir_all(&home);
    }
}
