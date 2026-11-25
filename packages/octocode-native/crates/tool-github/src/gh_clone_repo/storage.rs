//! Checkout destinations, process locks and atomic publication.
use super::{GitContext, GitError, check_control, hash};
use octocode_contract_types::ErrorCode;
use octocode_io::lock::{LockedFile, TryLock};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

static STAGE_ID: AtomicU64 = AtomicU64::new(0);

/// Root of every clone checkout under the Octocode home; local tools may read it.
pub fn clones_root(home: &Path) -> PathBuf {
    home.join("clones")
}

/// How deep each checkout sits under [`clones_root`]: `owner/repo/checkout`
/// ([`clone_dir`]).
pub const CHECKOUT_DEPTH: usize = 3;

/// Written last by a store's init: a store without it is unfinished.
const STORE_READY: &str = "octocode-store-ready";

/// One blob-filtered store per host and repository. A distinct namespace
/// keeps older shallow stores and their published worktrees usable. Recovery
/// advances the generation without changing a store with live worktrees.
pub(super) struct StoreKey {
    parent: PathBuf,
    stem: String,
}

impl StoreKey {
    pub(super) fn new(home: &Path, owner: &str, repo: &str, host: &str) -> Self {
        let stem = format!(
            "{}__full__host_{}",
            repo.to_ascii_lowercase(),
            hash(host, 16)
        );
        Self {
            parent: octocode_tools_shared::gh_git::store_root(home)
                .join(owner.to_ascii_lowercase()),
            stem,
        }
    }

    /// The store of `generation`.
    pub(super) fn dir(&self, generation: u64) -> PathBuf {
        self.parent
            .join(format!("{}__g{generation}.git", self.stem))
    }

    /// What the store lock is keyed by: one path for every generation.
    pub(super) fn anchor(&self) -> PathBuf {
        self.parent.join(&self.stem)
    }

    /// The generations that exist on disk, ascending.
    pub(super) fn generations(&self) -> Vec<u64> {
        let prefix = format!("{}__g", self.stem);
        let mut found = fs::read_dir(&self.parent)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                name.to_str()?
                    .strip_prefix(&prefix)?
                    .strip_suffix(".git")?
                    .parse::<u64>()
                    .ok()
            })
            .collect::<Vec<_>>();
        found.sort_unstable();
        found
    }

    /// The store a checkout is made in: the newest whose init finished, else
    /// the newest unfinished one (its init resumes), else the first.
    pub(super) fn current(&self) -> PathBuf {
        self.ready()
            .or_else(|| self.generations().last().map(|newest| self.dir(*newest)))
            .unwrap_or_else(|| self.dir(1))
    }

    /// The newest store whose init finished.
    pub(super) fn ready(&self) -> Option<PathBuf> {
        self.generations()
            .into_iter()
            .rev()
            .map(|generation| self.dir(generation))
            .find(|dir| is_ready(dir))
    }

    /// Where a store is made: the generation after the newest that exists.
    pub(super) fn next_dir(&self) -> PathBuf {
        self.dir(self.generations().last().map_or(1, |newest| newest + 1))
    }
}

/// Create the directories above `store`, owner-only.
pub(super) fn create_parent(store: &Path) -> Result<(), GitError> {
    match store.parent() {
        Some(parent) => create_private_dir_all(parent).map_err(storage_io),
        None => Ok(()),
    }
}

/// Whether `checkout` is a linked worktree of a store under `home`: its
/// `.git` is a file naming a git directory inside the store root. Anything
/// else at a destination (a standalone checkout of an earlier release, a
/// `.git` file pointing elsewhere) is not reusable.
pub(super) fn is_store_worktree(home: &Path, checkout: &Path) -> bool {
    let is_file = fs::symlink_metadata(checkout.join(".git")).is_ok_and(|meta| meta.is_file());
    let Some(git_dir) = git_dir(checkout).filter(|_| is_file) else {
        return false;
    };
    let root = octocode_tools_shared::gh_git::store_root(home);
    git_dir.starts_with(&root)
        || root
            .canonicalize()
            .is_ok_and(|root| git_dir.starts_with(root))
}

/// The store whose administration tracks the linked worktree `checkout`
/// ([`is_store_worktree`]): its git directory is `<store>/worktrees/<id>`.
pub(super) fn worktree_store(home: &Path, checkout: &Path) -> Option<PathBuf> {
    if !is_store_worktree(home, checkout) {
        return None;
    }
    store_of_admin(&git_dir(checkout)?).map(Path::to_path_buf)
}

/// The store a worktree administration directory `<store>/worktrees/<id>`
/// belongs to.
fn store_of_admin(admin: &Path) -> Option<&Path> {
    let worktrees = admin.parent()?;
    (worktrees.file_name()? == "worktrees")
        .then(|| worktrees.parent())
        .flatten()
}

/// The store of `key` (under the current home) that a checkout's `.git`
/// pointer names by directory name, when the pointer does not resolve: the
/// administration paths are absolute, so a moved `OCTOCODE_HOME` leaves
/// every checkout pointing into the old one. `None` for a pointer that
/// resolves, a standalone checkout, or a store this home does not hold.
pub(super) fn moved_store(key: &StoreKey, checkout: &Path) -> Option<PathBuf> {
    let is_file = fs::symlink_metadata(checkout.join(".git")).is_ok_and(|meta| meta.is_file());
    let admin = git_dir(checkout).filter(|admin| is_file && !admin.exists())?;
    let name = store_of_admin(&admin)?.file_name()?;
    key.generations()
        .into_iter()
        .map(|generation| key.dir(generation))
        .find(|store| store.file_name() == Some(name))
}

/// Whether no worktree is registered in `store` (its administration
/// directory is empty or absent).
pub(super) fn has_no_worktrees(store: &Path) -> bool {
    fs::read_dir(store.join("worktrees")).is_ok_and(|mut entries| entries.next().is_none())
        || !store.join("worktrees").exists()
}

/// Whether `store` finished its init ([`mark_ready`]).
pub(super) fn is_ready(store: &Path) -> bool {
    store.join(STORE_READY).is_file()
}

/// A fetch may have written objects before it failed or before its process
/// stopped. Only a completed filtered fetch may supply future checkouts.
const HISTORY_VERIFIED: &str = "octocode-history-verified";

pub(super) fn has_verified_history(store: &Path) -> bool {
    store.join(HISTORY_VERIFIED).is_file() && !store.join("shallow").exists()
}

pub(super) fn begin_history_fetch(store: &Path) -> Result<(), GitError> {
    match fs::remove_file(store.join(HISTORY_VERIFIED)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result.map_err(storage_io),
    }
}

pub(super) fn verify_history_fetch(store: &Path) -> Result<(), GitError> {
    if store
        .join("shallow")
        .try_exists()
        .map_err(storage_read_io)?
    {
        return Err(GitError::new(
            ErrorCode::GitFailed,
            "The source supplied shallow history; the checkout requires complete ancestry.",
        ));
    }
    octocode_io::atomic::write_atomic(&store.join(HISTORY_VERIFIED), b"", true).map_err(storage_io)
}

/// The text of the config file `path` of a store.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "the config `git init --bare` just wrote in a store octocode owns, from the user's own git templates: it is rewritten whole, so a bounded read would refuse a valid store"
    )
)]
pub(super) fn read_store_file(path: &Path) -> Result<String, GitError> {
    fs::read_to_string(path).map_err(storage_read_io)
}

/// Replace the config file `path` of a store with `text`.
pub(super) fn write_store_file(path: &Path, text: &str) -> Result<(), GitError> {
    octocode_io::atomic::write_atomic(path, text.as_bytes(), false).map_err(storage_io)
}

/// The commit the detached HEAD of the linked worktree `checkout` names,
/// read from its administration directory; `None` when that cannot be read
/// or HEAD is no commit id.
pub(super) fn detached_head(checkout: &Path) -> Option<String> {
    // A commit id is at most 64 hex digits (SHA-256) plus a line ending, so a
    // longer HEAD names no commit and is not read further.
    const HEAD_BYTES: u64 = 66;
    let file = fs::File::open(git_dir(checkout)?.join("HEAD")).ok()?;
    let head = octocode_io::read::read_limited(file, HEAD_BYTES).ok()??;
    let head = std::str::from_utf8(&head).ok()?.trim();
    octocode_github::is_full_sha(head).then(|| head.to_ascii_lowercase())
}

/// How many worktrees are registered in `store`, live or gone.
pub(super) fn registered_worktrees(store: &Path) -> usize {
    fs::read_dir(store.join("worktrees")).map_or(0, |entries| entries.flatten().count())
}

/// Record that `store` finished its init.
pub(super) fn mark_ready(store: &Path) -> Result<(), GitError> {
    octocode_io::atomic::write_atomic(&store.join(STORE_READY), b"", true).map_err(storage_io)
}

// GitHub owner/repo names are case-insensitive and share one destination.
fn repo_dir(home: &Path, owner: &str, repo: &str) -> PathBuf {
    clones_root(home)
        .join(owner.to_ascii_lowercase())
        .join(repo.to_ascii_lowercase())
}

/// The destination of `commit` (a full SHA, keyed lowercase: one commit
/// in any case is one checkout) with its sparse set. Every checkout holds
/// the commit's whole ancestry, so history keys nothing: this is the very
/// entry an earlier build gave a row without a requested depth, so its
/// checkouts are still found and reused. An entry an earlier build keyed by
/// a depth (`__d<n>` before `__host_`) is no row's destination: it stays on
/// disk, readable and registered in its store, beside this one.
pub(super) fn clone_dir(
    home: &Path,
    owner: &str,
    repo: &str,
    commit: &str,
    sparse_key: Option<&str>,
    host: &str,
) -> PathBuf {
    let sparse = sparse_key
        .map(|key| format!("__sp_{}", hash(key, 6)))
        .unwrap_or_default();
    repo_dir(home, owner, repo).join(format!(
        "{}__full{sparse}__host_{}",
        commit.to_ascii_lowercase(),
        hash(host, 16)
    ))
}

/// The lock file of the checkout `clone_dir`. It is never removed: a writer
/// waiting on it must not lock a file another writer recreated.
pub(super) fn lock_path(home: &Path, clone_dir: &Path) -> PathBuf {
    home.join("tmp")
        .join("clone-locks")
        .join(format!("{}.lock", hash(&clone_dir.to_string_lossy(), 16)))
}

/// A held checkout lock: the OS lock ghStructure's materialized snapshots
/// use ([`octocode_io::lock`]), so it spans git subprocesses and other
/// processes, and the kernel releases a crashed holder's lock. A waiting row
/// waits as long as the holder lives: the holder's own git steps are bounded
/// and a dead holder released the lock, so only the waiter's own deadline or
/// cancellation ends the wait (no fixed wait fails a row whose work is valid).
pub(super) struct CloneLock {
    _held: LockedFile,
}

impl CloneLock {
    pub(super) fn acquire(clone_dir: &Path, context: &GitContext<'_>) -> Result<Self, GitError> {
        let path = lock_path(&context.config.home, clone_dir);
        let mut file = open_lock(&path).map_err(storage_io)?;
        loop {
            check_control(context)?;
            match octocode_io::lock::try_lock(file).map_err(storage_io)? {
                TryLock::Locked(lock) => return Ok(Self { _held: lock }),
                TryLock::Busy(busy) => file = busy,
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// The lock of the checkout `clone_dir` if nobody holds it, else `None`: a
/// checkout whose lock is held is in use. `Err` when the lock file cannot be
/// opened or the filesystem refuses locks.
pub(super) fn try_lock_checkout(
    home: &Path,
    clone_dir: &Path,
) -> Result<Option<LockedFile>, GitError> {
    let file = open_lock(&lock_path(home, clone_dir)).map_err(storage_io)?;
    match octocode_io::lock::try_lock(file).map_err(storage_io)? {
        TryLock::Locked(lock) => Ok(Some(lock)),
        TryLock::Busy(_) => Ok(None),
    }
}

/// Create `path` and its missing parents readable by the owner only: a clone
/// holds code the user could read with their own token, so a directory this
/// tool creates (under whatever home exists) is never group- or world-readable.
pub(super) fn create_private_dir_all(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

fn open_lock(path: &Path) -> io::Result<fs::File> {
    if let Some(parent) = path.parent() {
        create_private_dir_all(parent)?;
    }
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
}

pub(super) fn stage_dir(home: &Path, clone_dir: &Path) -> Result<PathBuf, GitError> {
    let base = octocode_tools_shared::gh_git::stage_root(home);
    create_private_dir_all(&base).map_err(storage_io)?;
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

/// Rename the directory `from` to `to`: how [`promote`] moves a checkout
/// that no worktree administration tracks.
pub(super) fn rename_dir(from: &Path, to: &Path) -> Result<(), GitError> {
    fs::rename(from, to).map_err(storage_io)
}

/// Publish `stage` as `destination`: the previous destination is moved aside,
/// `place` puts the stage there, and the previous one is removed; when
/// `place` fails the previous destination is restored. `relocate` moves the
/// previous destination, aside and back: a linked worktree moves through git
/// so its registration follows it, whatever happens to the stage.
pub(super) fn promote(
    stage: &Path,
    destination: &Path,
    relocate: impl Fn(&Path, &Path) -> Result<(), GitError>,
    place: impl FnOnce() -> Result<(), GitError>,
) -> Result<(), GitError> {
    let parent = destination.parent().ok_or_else(|| {
        GitError::new(
            ErrorCode::FileAccessFailed,
            "Clone destination has no parent",
        )
    })?;
    create_private_dir_all(parent).map_err(storage_io)?;
    let previous = stage.with_extension("previous");
    remove_dir(&previous);
    let had_previous = destination.exists();
    if had_previous {
        relocate(destination, &previous)?;
    }
    if let Err(error) = place() {
        if had_previous && let Err(restore) = relocate(&previous, destination) {
            return Err(write_denied(GitError::new(
                ErrorCode::FileAccessFailed,
                format!(
                    "Clone publication failed ({}); previous checkout remains at '{}' because rollback failed ({}).",
                    error.message,
                    previous.display(),
                    restore.message
                ),
            )));
        }
        return Err(error);
    }
    remove_dir(&previous);
    Ok(())
}

/// Remove the stages under `tmp/clone-tmp` whose creating process is gone:
/// a row that crashed mid-checkout leaves its stage (named for the process)
/// registered in its store, and a store with a registered worktree is never
/// retired. No live row uses a dead process's stage, whichever store it
/// belongs to; `git worktree prune` then drops its registration. A stage of
/// this process or of a live one is left alone.
pub(super) fn remove_crashed_stages(home: &Path) {
    let Ok(stages) = fs::read_dir(octocode_tools_shared::gh_git::stage_root(home)) else {
        return;
    };
    for stage in stages.flatten().map(|entry| entry.path()) {
        if stage_owner(&stage).is_some_and(|pid| {
            pid != std::process::id() && !octocode_tools_shared::gh_git::process::process_alive(pid)
        }) {
            remove_dir(&stage);
        }
    }
}

/// The process that made `stage` ([`stage_dir`] names it
/// `<hash>-<name>-<pid>-<id>`).
fn stage_owner(stage: &Path) -> Option<u32> {
    let name = stage.file_name()?.to_string_lossy();
    name.rsplit('-').nth(1)?.parse().ok()
}

pub(super) fn remove_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

/// The completion stamp inside a checkout's git directory (its worktree
/// administration directory in the store): Git ignores it, so the checkout
/// stays clean, and it travels with the stage when published.
const COMPLETED_AT_FILE: &str = "octocode-cloned-at";

/// Where `checkout` keeps its git data ([`checkout_git_dir`](octocode_engine::search::walk::checkout_git_dir)).
fn git_dir(checkout: &Path) -> Option<PathBuf> {
    octocode_engine::search::walk::checkout_git_dir(checkout)
}

/// The last-use marker inside a checkout's git directory: its modification
/// time is when the checkout was last cloned or reused. Git ignores it, so the
/// checkout stays clean, and it travels with the stage when published.
const LAST_USED_FILE: &str = "octocode-last-used";

/// Record that the checkout `path` (whose lock the caller holds) was used at `at`.
pub(super) fn record_use(path: &Path, at: SystemTime) -> Result<(), GitError> {
    let git_dir = git_dir(path).ok_or_else(|| {
        GitError::new(
            ErrorCode::FileAccessFailed,
            "Clone checkout has no git directory to record its use in.",
        )
    })?;
    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(git_dir.join(LAST_USED_FILE))
        .and_then(|marker| marker.set_modified(at))
        .map_err(storage_io)
}

/// When the checkout `path` was last used ([`record_use`]); a checkout
/// without a marker is as recent as its completion stamp, or else as its
/// directory.
pub(super) fn last_used(path: &Path) -> SystemTime {
    let modified = |file: &Path| fs::metadata(file).and_then(|meta| meta.modified()).ok();
    git_dir(path)
        .and_then(|git_dir| {
            modified(&git_dir.join(LAST_USED_FILE))
                .or_else(|| modified(&git_dir.join(COMPLETED_AT_FILE)))
        })
        .or_else(|| modified(path))
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Record that the worktree `stage` was completed now; returns that time.
pub(super) fn record_completion(stage: &Path) -> Result<String, GitError> {
    // To the second, as every other time a reply states.
    let at = octocode_runtime_base::civil_date::iso8601_secs(
        octocode_runtime_base::civil_date::unix_secs(),
    );
    let git_dir = git_dir(stage).ok_or_else(|| {
        GitError::new(
            ErrorCode::FileAccessFailed,
            "Clone checkout has no git directory to record its completion in.",
        )
    })?;
    fs::write(git_dir.join(COMPLETED_AT_FILE), &at).map_err(storage_io)?;
    Ok(at)
}

/// Record what the checkout at `checkout` holds for the local tools
/// ([`clone_scope`](octocode_tools_shared::gh_git::clone_scope)), unless it
/// already says so.
pub(super) fn record_scope(
    checkout: &Path,
    record: &octocode_tools_shared::gh_git::clone_scope::CloneRecord,
) -> Result<(), GitError> {
    let git_dir = git_dir(checkout).ok_or_else(|| {
        GitError::new(
            ErrorCode::FileAccessFailed,
            "Clone checkout has no git directory to record its scope in.",
        )
    })?;
    let scope = octocode_tools_shared::gh_git::clone_scope::read_record(&git_dir);
    if scope.as_ref() == Some(record) {
        return Ok(());
    }
    octocode_tools_shared::gh_git::clone_scope::write_record(&git_dir, record).map_err(storage_io)
}

/// When the checkout was completed ([`record_completion`]); `None` for a
/// checkout made before the stamp existed.
pub(super) fn completed_at(path: &Path) -> Option<String> {
    fs::read_to_string(git_dir(path)?.join(COMPLETED_AT_FILE))
        .ok()
        .map(|text| to_the_second(text.trim()))
        .filter(|text| !text.is_empty())
}

/// A recorded time to the second (`…:16.158Z` reads `…:16Z`): a clone an
/// earlier build recorded in milliseconds states the form every other time
/// does.
fn to_the_second(at: &str) -> String {
    match at.split_once('.') {
        Some((seconds, fraction)) if fraction.ends_with('Z') => format!("{seconds}Z"),
        _ => at.to_owned(),
    }
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

/// A clone-storage read that failed.
fn storage_read_io(error: io::Error) -> GitError {
    GitError::new(
        ErrorCode::FileAccessFailed,
        format!("Clone storage operation failed: {error}"),
    )
}

/// A write, lock or move under the clones root that failed: its recovery is
/// a writable home with free space, not read permissions.
fn storage_io(error: io::Error) -> GitError {
    write_denied(storage_read_io(error))
}

/// `error`, a write under the Octocode home that failed, with the recovery
/// for a full or read-only home.
pub(super) fn write_denied(mut error: GitError) -> GitError {
    error
        .hints
        .push(octocode_runtime_base::response::rows::WRITE_ACCESS_HINT.to_owned());
    error
}

#[cfg(test)]
mod tests;
