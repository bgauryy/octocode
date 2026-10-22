use super::{CloneContext, CloneError, check_control, hash};
use crate::cache::evictions::log_eviction;
use crate::civil_date::{iso8601_millis, rfc3339_millis, unix_millis};
use octocode_contract_types::ErrorCode;
use octocode_io::atomic::write_atomic;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

pub(super) const META_FILE: &str = ".octocode-clone-meta.json";
pub(super) const LOCK_META_FILE: &str = ".octocode-lock.json";
const STALE_LOCK_AGE: Duration = Duration::from_secs(5 * 60);
const STALE_ARTIFACT_AGE: Duration = Duration::from_secs(15 * 60);
static STAGE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CacheMeta {
    pub cloned_at: String,
    pub expires_at: String,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub commit_sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparse_path: Option<String>,
    pub source: String,
    /// The whole entry's bytes (checkout and `.git`) at publication, which
    /// eviction weighs it by without walking it.
    pub size_bytes: u64,
    /// Checked-out bytes outside `.git` at publication: a clean checkout's
    /// size cannot change, so a hit reads it instead of walking the tree.
    pub checkout_bytes: u64,
    /// Commits of history fetched.
    pub depth: u64,
    /// Repo-relative paths of the repository symlinks the checkout holds
    /// (as plain files), recorded at publication so a hit reports them.
    pub symlinks: Vec<String>,
}
// Every field is required: a meta written by an older layout fails to
// parse, so `valid_clone` rejects it and the entry is cloned again or
// evicted, never served with facts it does not record.

/// What a cache entry holds: repository, commit, sparse scope
/// ([`super::Sparse::key`]), and history depth.
pub(super) struct Identity<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    /// The full commit SHA checked out (recorded as the meta's `branch`).
    pub commit: &'a str,
    pub sparse_key: Option<&'a str>,
    pub depth: u64,
}

impl CacheMeta {
    /// The meta of a fresh, verified checkout of `identity` whose recorded
    /// sizes and symlinks are `facts`.
    pub fn new(
        identity: &Identity<'_>,
        commit_sha: &str,
        ttl: Duration,
        facts: CheckoutFacts,
    ) -> Self {
        let now = unix_millis();
        Self {
            cloned_at: iso8601_millis(now),
            expires_at: iso8601_millis(now.saturating_add(ttl.as_millis() as i64)),
            owner: identity.owner.to_owned(),
            repo: identity.repo.to_owned(),
            branch: identity.commit.to_owned(),
            commit_sha: commit_sha.to_owned(),
            sparse_path: identity.sparse_key.map(str::to_owned),
            source: "clone".into(),
            size_bytes: facts.size_bytes,
            checkout_bytes: facts.checkout_bytes,
            depth: identity.depth,
            symlinks: facts.symlinks,
        }
    }
}

/// What a published checkout records about itself: its sizes and symlinks.
#[cfg_attr(test, derive(Default))]
pub(super) struct CheckoutFacts {
    pub size_bytes: u64,
    pub checkout_bytes: u64,
    pub symlinks: Vec<String>,
}

impl CacheMeta {
    /// Whether this persisted checkout describes the requested identity. A
    /// cache directory whose meta disagrees (hash collision, manual edit,
    /// layout drift) must be re-cloned rather than served.
    pub fn matches(&self, identity: &Identity<'_>) -> bool {
        self.owner.eq_ignore_ascii_case(identity.owner)
            && self.repo.eq_ignore_ascii_case(identity.repo)
            && self.branch.eq_ignore_ascii_case(identity.commit)
            && self.sparse_path.as_deref() == identity.sparse_key
            && self.depth == identity.depth
    }
}

/// The directory holding every checkout of `owner/repo`. GitHub owner/repo
/// names are case-insensitive: Foo/Bar and foo/bar are one repository and
/// must share one checkout.
/// Root of every clone checkout under the cache home; local tools may read it.
pub(crate) fn clones_root(home: &Path) -> PathBuf {
    home.join("tmp").join("clone")
}

fn repo_dir(home: &Path, owner: &str, repo: &str) -> PathBuf {
    clones_root(home)
        .join(owner.to_ascii_lowercase())
        .join(repo.to_ascii_lowercase())
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LockMeta {
    pid: u32,
    created_at: i64,
}

/// The cache entry of `commit` (a full SHA, keyed lowercase: one commit
/// in any case is one checkout) with its sparse set and history depth.
pub(super) fn clone_dir(
    home: &Path,
    owner: &str,
    repo: &str,
    commit: &str,
    sparse_key: Option<&str>,
    depth: u64,
    host: &str,
) -> PathBuf {
    let sparse = sparse_key
        .map(|key| format!("__sp_{}", hash(key, 6)))
        .unwrap_or_default();
    // A shallow checkout's entry names no depth.
    let history = if depth > 1 {
        format!("__d{depth}")
    } else {
        String::new()
    };
    repo_dir(home, owner, repo).join(format!(
        "{}{sparse}{history}__host_{}",
        commit.to_ascii_lowercase(),
        hash(host, 16)
    ))
}

pub(super) fn lock_dir(home: &Path, clone_dir: &Path) -> PathBuf {
    home.join("tmp")
        .join("clone-locks")
        .join(hash(&clone_dir.to_string_lossy(), 16))
}

pub(super) struct CloneLock {
    path: PathBuf,
}

impl CloneLock {
    pub fn acquire(clone_dir: &Path, context: &CloneContext<'_>) -> Result<Self, CloneError> {
        let path = lock_dir(&context.config.cache_home, clone_dir);
        let parent = path.parent().ok_or_else(|| {
            CloneError::new(ErrorCode::CacheUnavailable, "Clone lock path has no parent")
        })?;
        fs::create_dir_all(parent).map_err(cache_io)?;
        let started = std::time::Instant::now();
        loop {
            check_control(context)?;
            match fs::create_dir(&path) {
                Ok(()) => {
                    if let Err(error) = write_lock_meta(&path) {
                        let _ = fs::remove_dir_all(&path);
                        return Err(error);
                    }
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if recover_stale_lock(&path) {
                        continue;
                    }
                    if started.elapsed() >= context.config.lock_wait {
                        return Err(CloneError::new(
                            ErrorCode::Timeout,
                            format!(
                                "Timed out waiting for clone cache lock '{}'.",
                                path.display()
                            ),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => return Err(cache_io(error)),
            }
        }
    }
}

impl Drop for CloneLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn try_lock(path: &Path) -> Option<CloneLock> {
    fs::create_dir_all(path.parent()?).ok()?;
    match fs::create_dir(path) {
        Ok(()) => {
            if write_lock_meta(path).is_err() {
                let _ = fs::remove_dir_all(path);
                return None;
            }
            Some(CloneLock {
                path: path.to_owned(),
            })
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
        Err(_) => None,
    }
}

fn write_lock_meta(path: &Path) -> Result<(), CloneError> {
    let bytes = serde_json::to_vec(&LockMeta {
        pid: std::process::id(),
        created_at: unix_millis(),
    })
    .map_err(|error| CloneError::new(ErrorCode::CacheUnavailable, error.to_string()))?;
    write_atomic(&path.join(LOCK_META_FILE), &bytes, false).map_err(cache_io)
}

fn recover_stale_lock(path: &Path) -> bool {
    let recovery = path.with_extension("recovery");
    let Some(_claim) = try_lock(&recovery) else {
        return false;
    };
    let meta = fs::read(path.join(LOCK_META_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<LockMeta>(&bytes).ok());
    let stale = if let Some(meta) = meta {
        unix_millis().saturating_sub(meta.created_at) > STALE_LOCK_AGE.as_millis() as i64
            && !crate::process_status::is_alive(meta.pid)
    } else {
        fs::metadata(path)
            .and_then(|value| value.modified())
            .ok()
            .and_then(|value| SystemTime::now().duration_since(value).ok())
            .is_some_and(|age| age > STALE_LOCK_AGE)
    };
    if !stale {
        return false;
    }
    let tombstone = path.with_extension(format!("stale-{}", unix_millis()));
    fs::rename(path, &tombstone).is_ok() && fs::remove_dir_all(tombstone).is_ok()
}

pub(super) fn valid_clone(path: &Path, ttl: Duration) -> Option<CacheMeta> {
    let bytes = fs::read(path.join(META_FILE)).ok()?;
    let meta: CacheMeta = serde_json::from_slice(&bytes).ok()?;
    if meta.owner.trim().is_empty()
        || meta.repo.trim().is_empty()
        || meta.branch.trim().is_empty()
        || meta.source != "clone"
        || !octocode_github::is_full_sha(&meta.commit_sha)
    {
        return None;
    }
    let cloned = rfc3339_millis(&meta.cloned_at)?;
    let expires = rfc3339_millis(&meta.expires_at)?;
    let effective_expiry = cloned.saturating_add(ttl.as_millis() as i64).min(expires);
    (path.is_dir() && unix_millis() < effective_expiry).then_some(meta)
}

/// A served cache hit's age: when it was cloned and when it stops being
/// reused (the earlier of its written expiry and the current TTL).
pub(super) struct CacheAge {
    pub cloned_at: String,
    pub expires_at: String,
}

impl CacheAge {
    pub fn of(meta: &CacheMeta, ttl: Duration) -> Option<Self> {
        let cloned = rfc3339_millis(&meta.cloned_at)?;
        let expires = rfc3339_millis(&meta.expires_at)?;
        Some(Self {
            cloned_at: meta.cloned_at.clone(),
            expires_at: iso8601_millis(cloned.saturating_add(ttl.as_millis() as i64).min(expires)),
        })
    }
}

pub(super) fn write_meta(path: &Path, meta: &CacheMeta) -> Result<(), CloneError> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|error| CloneError::new(ErrorCode::CacheUnavailable, error.to_string()))?;
    let destination = path.join(META_FILE);
    // Metadata is written only in the hidden stage before directory promotion.
    // Exclusive creation protects source files arriving after the early check.
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&destination).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            metadata_conflict(&meta.owner, &meta.repo, &meta.commit_sha)
        } else {
            cache_io(error)
        }
    })?;
    file.write_all(&bytes).map_err(cache_io)
}

pub(super) fn metadata_conflict(owner: &str, repo: &str, commit_sha: &str) -> CloneError {
    CloneError {
        hints: vec![
            "Read path:\".octocode-clone-meta.json\" with ghGetFileContent at this ref, or browse the repository with ghStructure.".into(),
        ],
        ..CloneError::new(
            ErrorCode::CacheUnavailable,
            format!(
                "Repository {}/{}@{} contains reserved path '{}'; no checkout was replaced.",
                owner, repo, commit_sha, META_FILE,
            ),
        )
    }
}

pub(super) fn stage_dir(home: &Path, clone_dir: &Path) -> Result<PathBuf, CloneError> {
    let base = home.join("tmp").join("clone-tmp");
    fs::create_dir_all(&base).map_err(cache_io)?;
    let id = STAGE_ID.fetch_add(1, Ordering::Relaxed);
    let name = clone_dir.file_name().unwrap_or_default().to_string_lossy();
    let path = base.join(format!(
        "{}-{name}-{}-{id}",
        hash(&clone_dir.to_string_lossy(), 16),
        std::process::id()
    ));
    if path.exists() {
        log_eviction(home, "stage-preclean", &path, directory_size(&path));
    }
    remove_dir(&path);
    Ok(path)
}

pub(super) fn promote(home: &Path, stage: &Path, destination: &Path) -> Result<(), CloneError> {
    let parent = destination.parent().ok_or_else(|| {
        CloneError::new(
            ErrorCode::CacheUnavailable,
            "Clone destination has no parent",
        )
    })?;
    fs::create_dir_all(parent).map_err(cache_io)?;
    let previous = stage.with_extension("previous");
    remove_dir(&previous);
    let had_previous = destination.exists();
    if had_previous {
        log_eviction(
            home,
            "replaced-by-fresh-clone",
            destination,
            directory_size(destination),
        );
        fs::rename(destination, &previous).map_err(cache_io)?;
    }
    if let Err(error) = fs::rename(stage, destination) {
        if had_previous && let Err(restore) = fs::rename(&previous, destination) {
            return Err(CloneError::new(
                ErrorCode::CacheUnavailable,
                format!(
                    "Clone publication failed ({error}); previous checkout remains at '{}' because rollback failed ({restore}).",
                    previous.display()
                ),
            ));
        }
        return Err(cache_io(error));
    }
    remove_dir(&previous);
    Ok(())
}

pub(super) fn remove_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

pub(super) fn cleanup_stale_artifacts(home: &Path) {
    let Ok(entries) = fs::read_dir(home.join("tmp").join("clone-tmp")) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|value| value.modified())
            .ok()
            .and_then(|value| SystemTime::now().duration_since(value).ok())
            .is_some_and(|age| age > STALE_ARTIFACT_AGE);
        if old {
            let path = entry.path();
            log_eviction(home, "stale-artifact", &path, directory_size(&path));
            remove_dir(&path);
        }
    }
}

pub(super) fn evict(context: &CloneContext<'_>, protected: Option<&Path>) {
    let home = &context.config.cache_home;
    let ttl = context.config.cache_ttl;
    let max_bytes = context.config.max_cache_size_bytes;
    let max_count = context.config.max_clone_count;
    let base = clones_root(home);
    let mut live = vec![];
    for branch in clone_entries(home, &base) {
        if context.path_policy.validate(&branch).is_err() {
            continue;
        }
        let meta = if protected == Some(branch.as_path()) {
            valid_clone(&branch, ttl)
        } else {
            let lock = lock_dir(home, &branch);
            let Some(_guard) = try_lock(&lock) else {
                continue;
            };
            let meta = valid_clone(&branch, ttl);
            if meta.is_none() {
                // Expired/invalid bookkeeping does not authorize deleting the
                // caller's files. Unverifiable trees also remain untouched.
                if !super::git::checkout_status(context, &branch)
                    .is_ok_and(|status| status.safe_to_replace)
                {
                    continue;
                }
                log_eviction(home, "expired-or-invalid", &branch, directory_size(&branch));
                remove_dir(&branch);
            }
            meta
        };
        let Some(meta) = meta else { continue };
        live.push((
            branch.clone(),
            rfc3339_millis(&meta.cloned_at).unwrap_or(0),
            meta.size_bytes,
        ));
    }
    live.sort_by_key(|entry| entry.1);
    let mut bytes: u64 = live.iter().map(|entry| entry.2).sum();
    let mut count = live.len();
    for (path, _, size) in live {
        if bytes <= max_bytes && count <= max_count {
            break;
        }
        if protected == Some(path.as_path()) {
            continue;
        }
        let lock = lock_dir(home, &path);
        let Some(_guard) = try_lock(&lock) else {
            continue;
        };
        if !super::git::checkout_status(context, &path).is_ok_and(|status| status.safe_to_replace) {
            continue;
        }
        if fs::remove_dir_all(&path).is_ok() {
            log_eviction(home, "size-limit", &path, size);
            bytes = bytes.saturating_sub(size);
            count = count.saturating_sub(1);
        }
    }
}

/// Every checkout entry (`<owner>/<repo>/<entry>/`) under `base`. A file
/// beside the owner, repo or entry directories is no entry and nothing reads
/// it (a retired layout's `.default-branch__host_*.json`): it is removed as
/// the walk passes it.
fn clone_entries(home: &Path, base: &Path) -> Vec<PathBuf> {
    let mut values = vec![];
    for owner in directories(home, base) {
        for repo in directories(home, &owner) {
            values.extend(directories(home, &repo));
        }
    }
    values
}

/// The directories directly under `parent`; every other entry there (a
/// file or a symlink, never followed) is an orphan and is removed.
fn directories(home: &Path, parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => directories.push(path),
            Ok(_) => {
                let size = fs::symlink_metadata(&path).map_or(0, |meta| meta.len());
                if fs::remove_file(&path).is_ok() {
                    log_eviction(home, "orphaned-file", &path, size);
                }
            }
            Err(_) => {}
        }
    }
    directories
}

/// Bytes of a checkout's `.git` directory.
pub(super) fn git_dir_size(path: &Path) -> u64 {
    directory_size(&path.join(".git"))
}

pub(super) fn checked_out_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name() != ".git")
        .map(|entry| entry.path())
        .map(|path| directory_size(&path))
        .sum()
}

fn directory_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.file_type().is_symlink() {
        return 0;
    }
    if meta.is_file() {
        return meta.len();
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

fn cache_io(error: io::Error) -> CloneError {
    CloneError::new(
        ErrorCode::CacheUnavailable,
        format!("Clone cache operation failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_cache_rejects_malformed_timestamps() {
        for bad in ["2026-01-01T00x00y00z000Z", "2026-02-31T00:00:00.000Z"] {
            let sha = "a".repeat(40);
            let mut meta = CacheMeta::new(
                &identity("o", "r", &sha, None, 1),
                &sha,
                Duration::from_secs(60),
                CheckoutFacts::default(),
            );
            meta.cloned_at = bad.into();
            assert!(
                CacheAge::of(&meta, Duration::from_secs(60)).is_none(),
                "{bad}"
            );
        }
    }

    #[test]
    fn timestamp_round_trip() {
        for value in [0, 1_700_000_000_123, 1_900_000_000_999] {
            assert_eq!(rfc3339_millis(&iso8601_millis(value)), Some(value));
        }
        assert_eq!(rfc3339_millis("2024-01-01T00:00:00.00éZ"), None);
    }

    #[test]
    fn commit_cache_keys_ignore_case() {
        let home = Path::new("/tmp/octocode-cache-root");
        let sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        let entry = |owner, repo, commit| {
            clone_dir(home, owner, repo, commit, None, 1, "https://example.test")
        };
        let lower = sha.to_ascii_lowercase();
        assert_eq!(entry("o", "r", sha), entry("o", "r", &lower));
        // Owner/repo are case-insensitive on GitHub: one checkout.
        assert_eq!(entry("Octo", "Repo", sha), entry("octo", "repo", sha));
        assert!(entry("o", "r", sha).starts_with(home.join("tmp/clone/o/r")));
    }

    fn identity<'a>(
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
        sparse_key: Option<&'a str>,
        depth: u64,
    ) -> Identity<'a> {
        Identity {
            owner,
            repo,
            commit,
            sparse_key,
            depth,
        }
    }

    #[test]
    fn cache_meta_matches_only_the_requested_identity() {
        let (a, b) = ("a".repeat(40), "b".repeat(40));
        let meta = CacheMeta::new(
            &identity("Owner", "Repo", &a, Some("src"), 1),
            &a,
            Duration::from_secs(60),
            CheckoutFacts::default(),
        );
        assert!(meta.matches(&identity("owner", "repo", &a, Some("src"), 1)));
        assert!(meta.matches(&identity("owner", "repo", &"A".repeat(40), Some("src"), 1)));
        assert!(!meta.matches(&identity("owner", "other", &a, Some("src"), 1)));
        assert!(!meta.matches(&identity("owner", "repo", &b, Some("src"), 1)));
        assert!(!meta.matches(&identity("owner", "repo", &a, None, 1)));
        assert!(!meta.matches(&identity("owner", "repo", &a, Some("src"), 5)));
    }

    #[test]
    fn deep_history_and_sparse_sets_key_their_own_entries() {
        let home = Path::new("/tmp/octocode-cache-root");
        let sha = "c".repeat(40);
        let shallow = clone_dir(home, "o", "r", &sha, None, 1, "https://example.test");
        let deep = clone_dir(home, "o", "r", &sha, None, 5, "https://example.test");
        assert_ne!(shallow, deep);
        assert!(deep.to_string_lossy().contains("__d5__host_"));
        crate::tools::test_support::checks::assert_absent(
            &serde_json::json!(shallow.to_string_lossy()),
            "__d",
        );
    }
}
