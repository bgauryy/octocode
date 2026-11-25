//! Byte cap on the checkouts under the clones root: once a new checkout is
//! published, the least recently used ones are removed until the total fits.
//!
//! It runs after a clone and not in the maintenance sweep: only a clone grows
//! the total, the sweep is gated to once a day, and the dirtiness check needs
//! the row's git runner and deadline. The shared object stores behind the
//! checkouts are not counted.
use super::{GitContext, git, storage};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

struct Entry {
    dir: PathBuf,
    size: u64,
    used: SystemTime,
}

/// Evict after a checkout was published (its lock is held by the caller, so
/// it stays), when `context.config` caps the checkouts. A checkout with local changes
/// stays, as does one whose commit cannot be read.
pub(super) fn after_clone(context: &GitContext<'_>) {
    let home = &context.config.home;
    let freed = evict_to_cap(home, context.config.clone_cap_bytes, |dir| {
        commit_of(dir)
            .is_some_and(|commit| git::safe_to_replace(context, dir, commit).unwrap_or(false))
    });
    if freed > 0 {
        // A log line, not a result: every notice kind is said once per process.
        #[expect(
            clippy::print_stderr,
            reason = "the bytes freed are logged on stderr, never in tool output; a notice would say only the first eviction of the process"
        )]
        {
            eprintln!(
                "octocode: ghCloneRepo evicted least recently used checkouts, freeing {freed} bytes (storage.cloneCacheLimit {} bytes)",
                context.config.clone_cap_bytes
            );
        }
    }
}

/// The commit a checkout directory is keyed by (`<commit>__full…`).
fn commit_of(dir: &Path) -> Option<&str> {
    let name = dir.file_name()?.to_str()?;
    name.split_once("__full").map(|(commit, _)| commit)
}

/// Remove checkouts under `home`, oldest use first, until their total size is
/// at most `cap` (0: no cap); returns the bytes freed. A checkout is removed
/// only under its own lock, so one in use stays, and only when `removable`
/// agrees (called under that lock). One that cannot be removed counts toward
/// the total, so the total may stay over the cap.
pub(super) fn evict_to_cap(home: &Path, cap: u64, removable: impl Fn(&Path) -> bool) -> u64 {
    if cap == 0 {
        return 0;
    }
    let mut entries = entries(home);
    let mut total = entries.iter().map(|entry| entry.size).sum::<u64>();
    entries.sort_by(|a, b| a.used.cmp(&b.used).then_with(|| a.dir.cmp(&b.dir)));
    let mut freed = 0;
    for entry in entries {
        if total <= cap {
            break;
        }
        if remove(home, &entry, &removable) {
            total = total.saturating_sub(entry.size);
            freed += entry.size;
        }
    }
    freed
}

fn remove(home: &Path, entry: &Entry, removable: &impl Fn(&Path) -> bool) -> bool {
    // An unreadable lock leaves the checkout: nothing proves it unused.
    let Ok(Some(_lock)) = storage::try_lock_checkout(home, &entry.dir) else {
        return false;
    };
    // A row that reused the checkout after the listing refreshed its use.
    if storage::last_used(&entry.dir) > entry.used || !removable(&entry.dir) {
        return false;
    }
    storage::remove_dir(&entry.dir);
    // Owner and repository folders that held only this checkout go with it.
    if let Some(repository) = entry.dir.parent() {
        let _ = fs::remove_dir(repository);
        if let Some(owner) = repository.parent() {
            let _ = fs::remove_dir(owner);
        }
    }
    !entry.dir.exists()
}

fn entries(home: &Path) -> Vec<Entry> {
    let children = |path: &Path| {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    };
    children(&storage::clones_root(home))
        .iter()
        .flat_map(|owner| children(owner))
        .flat_map(|repository| children(&repository))
        .map(|dir| Entry {
            size: storage::checked_out_size(&dir),
            used: storage::last_used(&dir),
            dir,
        })
        .collect()
}

#[cfg(test)]
mod tests;
