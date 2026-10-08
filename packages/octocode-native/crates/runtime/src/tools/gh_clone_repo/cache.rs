use super::{CloneContext, CloneError, check_control, hash};
use crate::cache::evictions::log_eviction;
use crate::civil_date::{days_from_civil, iso8601_millis};
use crate::private_file::write_atomic;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Checked-out bytes outside `.git` at publication: a clean checkout's
    /// size cannot change, so a hit reads it instead of walking the tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_bytes: Option<u64>,
    /// Whether the checkout was integrity-verified when it was created. Persisted
    /// so a cache hit reports the state it was created with instead of a bare
    /// `false`. Defaults to `false` for caches written before this field existed.
    #[serde(default)]
    pub verified: bool,
    /// Commits of history fetched; entries written before `depth` existed
    /// are shallow.
    #[serde(default = "shallow")]
    pub depth: u64,
}

fn shallow() -> u64 {
    1
}

/// What a cache entry holds: repository, ref, sparse scope ([`super::Sparse::key`]),
/// and history depth.
pub(super) struct Identity<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub branch: &'a str,
    pub sparse_key: Option<&'a str>,
    pub depth: u64,
}

impl CacheMeta {
    pub fn new(identity: &Identity<'_>, commit_sha: &str, ttl: Duration) -> Self {
        let now = now_millis();
        Self {
            cloned_at: iso8601_millis(now),
            expires_at: iso8601_millis(now.saturating_add(ttl.as_millis() as i64)),
            owner: identity.owner.to_owned(),
            repo: identity.repo.to_owned(),
            branch: identity.branch.to_owned(),
            commit_sha: commit_sha.to_owned(),
            sparse_path: identity.sparse_key.map(str::to_owned),
            source: "clone".into(),
            size_bytes: None,
            checkout_bytes: None,
            // new() is only called on the fresh-clone path, which verifies the
            // checkout before reporting success.
            verified: true,
            depth: identity.depth,
        }
    }
}

impl CacheMeta {
    /// Whether this persisted checkout describes the requested identity. A
    /// cache directory whose meta disagrees (hash collision, manual edit,
    /// layout drift) must be re-cloned rather than served.
    pub fn matches(&self, identity: &Identity<'_>) -> bool {
        let branch_matches = if octocode_github::is_full_sha(identity.branch) {
            self.branch.eq_ignore_ascii_case(identity.branch)
        } else {
            self.branch == identity.branch
        };
        self.owner.eq_ignore_ascii_case(identity.owner)
            && self.repo.eq_ignore_ascii_case(identity.repo)
            && branch_matches
            && self.sparse_path.as_deref() == identity.sparse_key
            && self.depth == identity.depth
    }
}

/// The default branch a default-branch clone resolved, per repository and
/// host. It lets an unbranched request find its cache entry without a GitHub
/// API call; it expires with the cache TTL, and `forceRefresh` ignores it.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultBranchAlias {
    branch: String,
    recorded_at: String,
}

fn alias_path(home: &Path, owner: &str, repo: &str, endpoint: &str) -> PathBuf {
    // A file beside the branch directories: eviction walks directories only.
    repo_dir(home, owner, repo).join(format!(".default-branch__host_{}.json", hash(endpoint, 16)))
}

/// The directory holding every checkout of `owner/repo`. GitHub owner/repo
/// names are case-insensitive: Foo/Bar and foo/bar are one repository and
/// must share one checkout.
fn repo_dir(home: &Path, owner: &str, repo: &str) -> PathBuf {
    home.join("tmp")
        .join("clone")
        .join(owner.to_ascii_lowercase())
        .join(repo.to_ascii_lowercase())
}

pub(super) fn default_branch_alias(
    home: &Path,
    owner: &str,
    repo: &str,
    endpoint: &str,
    ttl: Duration,
) -> Option<String> {
    let bytes = fs::read(alias_path(home, owner, repo, endpoint)).ok()?;
    let alias: DefaultBranchAlias = serde_json::from_slice(&bytes).ok()?;
    let recorded = parse_iso_millis(&alias.recorded_at)?;
    let fresh = now_millis() < recorded.saturating_add(ttl.as_millis() as i64);
    (fresh && !alias.branch.trim().is_empty()).then_some(alias.branch)
}

pub(super) fn write_default_branch_alias(
    home: &Path,
    owner: &str,
    repo: &str,
    endpoint: &str,
    branch: &str,
) {
    let path = alias_path(home, owner, repo, endpoint);
    let Ok(bytes) = serde_json::to_vec(&DefaultBranchAlias {
        branch: branch.to_owned(),
        recorded_at: iso8601_millis(now_millis()),
    }) else {
        return;
    };
    // Best effort: without an alias the next unbranched call clones again.
    let _ = write_atomic(&path, &bytes, false);
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LockMeta {
    pid: u32,
    created_at: i64,
}

pub(super) fn clone_dir(
    home: &Path,
    owner: &str,
    repo: &str,
    branch: &str,
    sparse_key: Option<&str>,
    depth: u64,
    endpoint: &str,
) -> PathBuf {
    // Full commit SHAs are case-insensitive; key them lowercase so FOO and foo
    // share one checkout.
    let lowered;
    let branch = if octocode_github::is_full_sha(branch) {
        lowered = branch.to_ascii_lowercase();
        lowered.as_str()
    } else {
        branch
    };
    let safe_branch =
        if branch == "." || branch == ".." || branch.contains('/') || branch.contains('\\') {
            format!(
                "{}__b_{}",
                branch.replace(['/', '\\'], "_"),
                hash(branch, 8)
            )
        } else {
            branch.to_owned()
        };
    let sparse = sparse_key
        .map(|key| format!("__sp_{}", hash(key, 6)))
        .unwrap_or_default();
    // Shallow entries keep the layout they had before `depth` existed.
    let history = if depth > 1 {
        format!("__d{depth}")
    } else {
        String::new()
    };
    repo_dir(home, owner, repo).join(format!(
        "{safe_branch}{sparse}{history}__host_{}",
        hash(endpoint, 16)
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
        let parent = path
            .parent()
            .ok_or_else(|| CloneError::new("cacheUnavailable", "Clone lock path has no parent"))?;
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
                            "timeout",
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
        created_at: now_millis(),
    })
    .map_err(|error| CloneError::new("cacheUnavailable", error.to_string()))?;
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
        now_millis().saturating_sub(meta.created_at) > STALE_LOCK_AGE.as_millis() as i64
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
    let tombstone = path.with_extension(format!("stale-{}", now_millis()));
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
    let cloned = parse_iso_millis(&meta.cloned_at)?;
    let expires = parse_iso_millis(&meta.expires_at)?;
    let effective_expiry = cloned.saturating_add(ttl.as_millis() as i64).min(expires);
    (path.is_dir() && now_millis() < effective_expiry).then_some(meta)
}

/// A served cache hit's age: when it was cloned and when it stops being
/// reused (the earlier of its written expiry and the current TTL).
pub(super) struct CacheAge {
    pub cloned_at: String,
    pub expires_at: String,
}

impl CacheAge {
    pub fn of(meta: &CacheMeta, ttl: Duration) -> Option<Self> {
        let cloned = parse_iso_millis(&meta.cloned_at)?;
        let expires = parse_iso_millis(&meta.expires_at)?;
        Some(Self {
            cloned_at: meta.cloned_at.clone(),
            expires_at: iso8601_millis(cloned.saturating_add(ttl.as_millis() as i64).min(expires)),
        })
    }
}

pub(super) fn write_meta(path: &Path, meta: &CacheMeta) -> Result<(), CloneError> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|error| CloneError::new("cacheUnavailable", error.to_string()))?;
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
            metadata_conflict(meta)
        } else {
            cache_io(error)
        }
    })?;
    file.write_all(&bytes).map_err(cache_io)
}

pub(super) fn metadata_conflict(meta: &CacheMeta) -> CloneError {
    CloneError {
        hints: vec![
            "Read path:\".octocode-clone-meta.json\" with ghGetFileContent at this ref, or browse the repository with ghStructure.".into(),
        ],
        ..CloneError::new(
            "cacheUnavailable",
            format!(
                "Repository {}/{}@{} contains reserved path '{}'; no checkout was replaced.",
                meta.owner, meta.repo, meta.commit_sha, META_FILE,
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
    let parent = destination
        .parent()
        .ok_or_else(|| CloneError::new("cacheUnavailable", "Clone destination has no parent"))?;
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
                "cacheUnavailable",
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
    for folder in ["clone-tmp", "tree-staging"] {
        let base = home.join("tmp").join(folder);
        let Ok(entries) = fs::read_dir(base) else {
            continue;
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
}

pub(super) fn evict(context: &CloneContext<'_>, protected: Option<&Path>) {
    let home = &context.config.cache_home;
    let ttl = context.config.cache_ttl;
    let max_bytes = context.config.max_cache_size_bytes;
    let max_count = context.config.max_clone_count;
    let base = home.join("tmp").join("clone");
    let mut live = vec![];
    for branch in clone_entries(&base) {
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
            parse_iso_millis(&meta.cloned_at).unwrap_or(0),
            meta.size_bytes.unwrap_or_else(|| directory_size(&branch)),
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

fn clone_entries(base: &Path) -> Vec<PathBuf> {
    let mut values = vec![];
    let Ok(owners) = fs::read_dir(base) else {
        return values;
    };
    for owner in owners.flatten().filter(|entry| entry.path().is_dir()) {
        let Ok(repos) = fs::read_dir(owner.path()) else {
            continue;
        };
        for repo in repos.flatten().filter(|entry| entry.path().is_dir()) {
            if let Ok(branches) = fs::read_dir(repo.path()) {
                values.extend(
                    branches
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| path.is_dir()),
                );
            }
        }
    }
    values
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
        "cacheUnavailable",
        format!("Clone cache operation failed: {error}"),
    )
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn parse_iso_millis(value: &str) -> Option<i64> {
    if !value.is_ascii()
        || value.len() != 24
        || &value[4..5] != "-"
        || &value[7..8] != "-"
        || &value[10..11] != "T"
        || &value[23..] != "Z"
    {
        return None;
    }
    let year = value[0..4].parse::<i64>().ok()?;
    let month = value[5..7].parse::<i64>().ok()?;
    let day = value[8..10].parse::<i64>().ok()?;
    let hour = value[11..13].parse::<i64>().ok()?;
    let minute = value[14..16].parse::<i64>().ok()?;
    let second = value[17..19].parse::<i64>().ok()?;
    let millis = value[20..23].parse::<i64>().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(
        days_from_civil(year, month, day) * 86_400_000
            + hour * 3_600_000
            + minute * 60_000
            + second * 1_000
            + millis,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_round_trip() {
        for value in [0, 1_700_000_000_123, 1_900_000_000_999] {
            assert_eq!(parse_iso_millis(&iso8601_millis(value)), Some(value));
        }
        assert_eq!(parse_iso_millis("2024-01-01T00:00:00.00éZ"), None);
    }

    #[test]
    fn unsafe_branch_names_remain_cache_leaves() {
        let home = Path::new("/tmp/octocode-cache-root");
        for branch in [".", "..", "feature/nested", r"feature\portable"] {
            let path = clone_dir(
                home,
                "owner",
                "repo",
                branch,
                None,
                1,
                "https://example.test",
            );
            assert!(path.starts_with(home.join("tmp/clone/owner/repo")));
            assert_ne!(
                path.file_name().and_then(|value| value.to_str()),
                Some(branch)
            );
        }
    }

    #[test]
    fn full_sha_cache_keys_ignore_case() {
        let home = Path::new("/tmp/octocode-cache-root");
        let sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        assert_eq!(
            clone_dir(home, "o", "r", sha, None, 1, "https://example.test"),
            clone_dir(
                home,
                "o",
                "r",
                &sha.to_ascii_lowercase(),
                None,
                1,
                "https://example.test"
            )
        );
        // Non-SHA refs stay case-sensitive (Git branch names are).
        assert_ne!(
            clone_dir(home, "o", "r", "Main", None, 1, "https://example.test"),
            clone_dir(home, "o", "r", "main", None, 1, "https://example.test")
        );
        // Owner/repo are case-insensitive on GitHub: one checkout.
        assert_eq!(
            clone_dir(
                home,
                "Octo",
                "Repo",
                "main",
                None,
                1,
                "https://example.test"
            ),
            clone_dir(
                home,
                "octo",
                "repo",
                "main",
                None,
                1,
                "https://example.test"
            )
        );
    }

    fn identity<'a>(
        owner: &'a str,
        repo: &'a str,
        branch: &'a str,
        sparse_key: Option<&'a str>,
        depth: u64,
    ) -> Identity<'a> {
        Identity {
            owner,
            repo,
            branch,
            sparse_key,
            depth,
        }
    }

    #[test]
    fn cache_meta_matches_only_the_requested_identity() {
        let meta = CacheMeta::new(
            &identity("Owner", "Repo", "main", Some("src"), 1),
            &"a".repeat(40),
            Duration::from_secs(60),
        );
        assert!(meta.matches(&identity("owner", "repo", "main", Some("src"), 1)));
        assert!(!meta.matches(&identity("owner", "other", "main", Some("src"), 1)));
        assert!(!meta.matches(&identity("owner", "repo", "dev", Some("src"), 1)));
        assert!(!meta.matches(&identity("owner", "repo", "main", None, 1)));
        assert!(!meta.matches(&identity("owner", "repo", "main", Some("src"), 5)));
        let pinned = CacheMeta::new(
            &identity("o", "r", &"b".repeat(40), None, 1),
            &"b".repeat(40),
            Duration::from_secs(60),
        );
        assert!(pinned.matches(&identity("o", "r", &"B".repeat(40), None, 1)));
    }

    #[test]
    fn meta_written_before_depth_existed_is_shallow() {
        let meta: CacheMeta = serde_json::from_value(serde_json::json!({
            "clonedAt": "2026-01-01T00:00:00.000Z",
            "expiresAt": "2026-01-02T00:00:00.000Z",
            "owner": "o", "repo": "r", "branch": "main",
            "commitSha": "a".repeat(40), "source": "clone"
        }))
        .expect("legacy meta");
        assert_eq!(meta.depth, 1);
        assert!(meta.matches(&identity("o", "r", "main", None, 1)));
    }

    #[test]
    fn deep_history_and_sparse_sets_key_their_own_entries() {
        let home = Path::new("/tmp/octocode-cache-root");
        let shallow = clone_dir(home, "o", "r", "main", None, 1, "https://example.test");
        let deep = clone_dir(home, "o", "r", "main", None, 5, "https://example.test");
        assert_ne!(shallow, deep);
        assert!(deep.to_string_lossy().contains("__d5__host_"));
        assert!(!shallow.to_string_lossy().contains("__d"));
    }

    #[test]
    fn default_branch_alias_round_trips_and_expires() {
        let home = std::env::temp_dir().join(format!(
            "octocode-alias-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let endpoint = "https://example.test";
        assert_eq!(
            default_branch_alias(&home, "O", "R", endpoint, Duration::from_secs(60)),
            None
        );
        fs::create_dir_all(home.join("tmp/clone/o/r")).expect("repo dir");
        write_default_branch_alias(&home, "O", "R", endpoint, "trunk");
        assert_eq!(
            default_branch_alias(&home, "o", "r", endpoint, Duration::from_secs(60)).as_deref(),
            Some("trunk")
        );
        assert_eq!(
            default_branch_alias(
                &home,
                "o",
                "r",
                "https://other.test",
                Duration::from_secs(60)
            ),
            None,
            "aliases are per host"
        );
        assert_eq!(
            default_branch_alias(&home, "o", "r", endpoint, Duration::ZERO),
            None,
            "an alias older than the TTL is ignored"
        );
        let _ = fs::remove_dir_all(&home);
    }
}
