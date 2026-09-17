use super::{CloneContext, CloneError, check_control, hash};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
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
}

impl CacheMeta {
    pub fn new(
        owner: &str,
        repo: &str,
        branch: &str,
        sparse_path: Option<&str>,
        commit_sha: &str,
        ttl: Duration,
    ) -> Self {
        let now = now_millis();
        Self {
            cloned_at: iso_millis(now),
            expires_at: iso_millis(now.saturating_add(ttl.as_millis() as i64)),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            branch: branch.to_owned(),
            commit_sha: commit_sha.to_owned(),
            sparse_path: sparse_path.map(str::to_owned),
            source: "clone".into(),
            size_bytes: None,
        }
    }
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
    sparse_path: Option<&str>,
    endpoint: &str,
) -> PathBuf {
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
    let sparse = sparse_path
        .map(|path| format!("__sp_{}", hash(path, 6)))
        .unwrap_or_default();
    home.join("tmp")
        .join("clone")
        .join(owner)
        .join(repo)
        .join(format!(
            "{safe_branch}{sparse}__host_{}",
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
        let parent = path.parent().ok_or_else(|| {
            CloneError::new("clone.cache.invalid", "Clone lock path has no parent")
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
                            "clone.cache.lockTimeout",
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
    .map_err(|error| CloneError::new("clone.cache.invalid", error.to_string()))?;
    write_private(&path.join(LOCK_META_FILE), &bytes).map_err(cache_io)
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
            && !process_alive(meta.pid)
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

fn process_alive(pid: u32) -> bool {
    crate::process_status::is_alive(pid)
}

pub(super) fn valid_clone(path: &Path, ttl: Duration) -> Option<CacheMeta> {
    let bytes = fs::read(path.join(META_FILE)).ok()?;
    let meta: CacheMeta = serde_json::from_slice(&bytes).ok()?;
    if meta.owner.trim().is_empty()
        || meta.repo.trim().is_empty()
        || meta.branch.trim().is_empty()
        || meta.source != "clone"
        || meta.commit_sha.len() != 40
        || !meta.commit_sha.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    let cloned = parse_iso_millis(&meta.cloned_at)?;
    let expires = parse_iso_millis(&meta.expires_at)?;
    let effective_expiry = cloned.saturating_add(ttl.as_millis() as i64).min(expires);
    (path.is_dir() && now_millis() < effective_expiry).then_some(meta)
}

pub(super) fn write_meta(path: &Path, meta: &CacheMeta) -> Result<(), CloneError> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|error| CloneError::new("clone.cache.invalid", error.to_string()))?;
    let destination = path.join(META_FILE);
    let temporary = path.join(format!("{META_FILE}.tmp-{}", std::process::id()));
    write_private(&temporary, &bytes).map_err(cache_io)?;
    fs::rename(&temporary, &destination).map_err(cache_io)
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
    remove_dir(&path);
    Ok(path)
}

pub(super) fn promote(stage: &Path, destination: &Path) -> Result<(), CloneError> {
    let parent = destination
        .parent()
        .ok_or_else(|| CloneError::new("clone.cache.invalid", "Clone destination has no parent"))?;
    fs::create_dir_all(parent).map_err(cache_io)?;
    let previous = stage.with_extension("previous");
    remove_dir(&previous);
    let had_previous = destination.exists();
    if had_previous {
        fs::rename(destination, &previous).map_err(cache_io)?;
    }
    if let Err(error) = fs::rename(stage, destination) {
        if had_previous && let Err(restore) = fs::rename(&previous, destination) {
            return Err(CloneError::new(
                "clone.cache.rollbackFailed",
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
                remove_dir(&entry.path());
            }
        }
    }
}

pub(super) fn evict(
    home: &Path,
    ttl: Duration,
    max_bytes: u64,
    max_count: usize,
    protected: Option<&Path>,
) {
    let base = home.join("tmp").join("clone");
    let mut live = vec![];
    for branch in clone_entries(&base) {
        let meta = if protected == Some(branch.as_path()) {
            valid_clone(&branch, ttl)
        } else {
            let lock = lock_dir(home, &branch);
            let Some(_guard) = try_lock(&lock) else {
                continue;
            };
            let meta = valid_clone(&branch, ttl);
            if meta.is_none() {
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
        if fs::remove_dir_all(&path).is_ok() {
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
        "clone.cache.io",
        format!("Clone cache operation failed: {error}"),
    )
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
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

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_round_trip() {
        for value in [0, 1_700_000_000_123, 1_900_000_000_999] {
            assert_eq!(parse_iso_millis(&iso_millis(value)), Some(value));
        }
        assert_eq!(parse_iso_millis("2024-01-01T00:00:00.00éZ"), None);
    }

    #[test]
    fn unsafe_branch_names_remain_cache_leaves() {
        let home = Path::new("/tmp/octocode-cache-root");
        for branch in [".", "..", "feature/nested", r"feature\portable"] {
            let path = clone_dir(home, "owner", "repo", branch, None, "https://example.test");
            assert!(path.starts_with(home.join("tmp/clone/owner/repo")));
            assert_ne!(
                path.file_name().and_then(|value| value.to_str()),
                Some(branch)
            );
        }
    }
}
