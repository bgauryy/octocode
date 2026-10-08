//! Optional on-disk mirror of a key's blocking facts, so short-lived CLI
//! processes honor each other's limits.
use super::{KeyFacts, KeyState, now_ms};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct Persist {
    pub(super) path: PathBuf,
    pub(super) seen: Option<SystemTime>,
}

impl KeyFacts {
    /// Keep the most restrictive of two views (memory vs. disk).
    pub(super) fn merge(&mut self, other: KeyFacts, now_ms: u64) {
        for (name, bucket) in other.buckets {
            if bucket.remaining != 0 || bucket.reset.saturating_mul(1000) <= now_ms {
                continue;
            }
            let replace = self
                .buckets
                .get(&name)
                .is_none_or(|current| current.reset < bucket.reset || current.remaining > 0);
            if replace {
                self.buckets.insert(name, bucket);
            }
        }
        self.cooldown_until_ms = self.cooldown_until_ms.max(other.cooldown_until_ms);
        for (group, stamp) in other.last_start_ms {
            let entry = self.last_start_ms.entry(group).or_default();
            *entry = (*entry).max(stamp);
        }
        let mut window: Vec<u64> = self
            .code_search_ms
            .iter()
            .chain(other.code_search_ms.iter())
            .copied()
            .filter(|stamp| stamp.saturating_add(60_000) > now_ms)
            .collect();
        window.sort_unstable();
        window.dedup();
        self.code_search_ms = window.into();
    }

    /// Only facts that can block a future request are persisted.
    pub(super) fn blocking_view(&self, now_ms: u64) -> KeyFacts {
        KeyFacts {
            buckets: self
                .buckets
                .iter()
                .filter(|(_, b)| b.remaining == 0 && b.reset.saturating_mul(1000) > now_ms)
                .map(|(name, b)| (name.clone(), *b))
                .collect(),
            cooldown_until_ms: if self.cooldown_until_ms > now_ms {
                self.cooldown_until_ms
            } else {
                0
            },
            last_start_ms: self
                .last_start_ms
                .iter()
                .filter(|(_, stamp)| stamp.saturating_add(60_000) > now_ms)
                .map(|(group, stamp)| (group.clone(), *stamp))
                .collect(),
            code_search_ms: self
                .code_search_ms
                .iter()
                .copied()
                .filter(|stamp| stamp.saturating_add(60_000) > now_ms)
                .collect(),
            circuit_failures: 0,
            circuit_open_until_ms: 0,
        }
    }
}

impl KeyState {
    pub(super) fn attach_dir(&self, dir: &Path) {
        let mut persist = self
            .persist
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if persist.is_none() {
            *persist = Some(Persist {
                path: dir.join(self.key.file_name()),
                seen: None,
            });
        }
    }

    /// Merge the on-disk view when another process has written it since we
    /// last looked (one `stat` per logical request).
    pub fn refresh_from_disk(&self) {
        let mut persist = self
            .persist
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(persist) = persist.as_mut() else {
            return;
        };
        let Ok(modified) = std::fs::metadata(&persist.path).and_then(|meta| meta.modified()) else {
            return;
        };
        if persist.seen == Some(modified) {
            return;
        }
        persist.seen = Some(modified);
        let Some(disk) = std::fs::read(&persist.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<KeyFacts>(&bytes).ok())
        else {
            return;
        };
        self.facts().merge(disk, now_ms());
    }

    /// Load-merge-write via a unique temp file + atomic rename.
    pub(super) fn persist(&self) {
        self.persist_with(|_| {});
    }

    /// [`Self::persist`] with `adjust` applied to the merged view before the
    /// write. A merged view equal to the file is not written again (a repeat
    /// exhaust or cooldown with the same reset); a new start reservation
    /// always changes it, and it must reach the file, because it is what
    /// spaces the next process's search.
    pub(super) fn persist_with(&self, adjust: impl FnOnce(&mut KeyFacts)) {
        let path = {
            let persist = self
                .persist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match persist.as_ref() {
                Some(persist) => persist.path.clone(),
                None => return,
            }
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        // Serialize read-merge-write across processes: without it, two CLI
        // processes read the same file and the later rename drops the other's
        // start stamps and window slots (review L9). Released on drop. A
        // filesystem without lock support degrades to the unlocked write.
        let _lock = lock_file(&path);
        let now = now_ms();
        let mut view = self.facts().blocking_view(now);
        let on_disk = std::fs::read(&path).ok();
        if let Some(disk) = on_disk
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<KeyFacts>(bytes).ok())
        {
            view.merge(disk, now);
        }
        adjust(&mut view);
        let Ok(bytes) = serde_json::to_vec(&view) else {
            return;
        };
        if on_disk.as_deref() == Some(bytes.as_slice()) {
            return;
        }
        let tmp = parent.join(format!(
            ".{}.{}.{}.tmp",
            self.key.file_name(),
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        if let Ok(modified) = std::fs::metadata(&path).and_then(|meta| meta.modified())
            && let Some(persist) = self
                .persist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
        {
            persist.seen = Some(modified);
        }
    }
}

/// Exclusive advisory lock on the `<state>.lock` sidecar; `None` when the
/// file cannot be opened or the filesystem does not support locking.
fn lock_file(path: &Path) -> Option<std::fs::File> {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(PathBuf::from(name))
        .ok()?;
    file.lock().ok()?;
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn disk_state_round_trips_blocking_facts_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let reset = now_ms() / 1000 + 300;
        {
            let budget = GitHubBudget::relaxed();
            let state = budget.key_state(&key, Some(dir.path()));
            let mut headers = HeaderMap::new();
            headers.insert("x-ratelimit-remaining", "42".parse().expect("header"));
            headers.insert(
                "x-ratelimit-reset",
                reset.to_string().parse().expect("header"),
            );
            headers.insert("x-ratelimit-resource", "core".parse().expect("header"));
            state.observe(&headers, "core");
            state.exhaust("search", reset);
            state.cool_down(now_ms() + 30_000);
        }
        let file = dir.path().join(key.file_name());
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).expect("state file")).expect("json");
        assert!(raw["buckets"].get("core").is_none(), "{raw}");
        assert_eq!(raw["buckets"]["search"]["remaining"], 0);
        // A fresh process (new registry) sees the other process's facts.
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        let search = state.blocked("search", budget.config()).expect("search");
        assert_eq!(search.reset, Some(reset));
        let core = state.blocked("core", budget.config()).expect("cooldown");
        assert_eq!(core.reset, None);
        let leftovers = std::fs::read_dir(dir.path())
            .expect("dir")
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    /// Review L9: concurrent writers that do not share memory (separate
    /// processes; here separate registries) never drop each other's facts.
    #[test]
    fn concurrent_writers_keep_every_fact() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let now = now_ms();
        std::thread::scope(|scope| {
            for writer in 0..8 {
                let (dir, key) = (dir.path(), &key);
                scope.spawn(move || {
                    let budget = GitHubBudget::relaxed();
                    let state = budget.key_state(key, Some(dir));
                    for round in 0..25 {
                        state.persist_with(|view| {
                            view.last_start_ms.insert(format!("w{writer}-{round}"), now);
                        });
                    }
                });
            }
        });
        let disk: KeyFacts =
            serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
                .expect("json");
        assert_eq!(disk.last_start_ms.len(), 8 * 25);
    }

    /// A repeat of the same blocking fact leaves the file in place; a new
    /// fact replaces it.
    #[cfg(unix)]
    #[test]
    fn an_unchanged_view_is_not_rewritten() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        let file = dir.path().join(key.file_name());
        let inode = || std::fs::metadata(&file).expect("state file").ino();
        let reset = now_ms() / 1000 + 300;
        state.exhaust("search", reset);
        let first = inode();
        state.exhaust("search", reset);
        assert_eq!(inode(), first, "same view, no rewrite");
        state.cool_down(now_ms() + 30_000);
        assert_ne!(inode(), first, "a new cooldown is written");
    }
}
