//! Shared 24-hour sweep of native tmp caches.
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MARKER: &str = ".last-cache-maintenance";

/// Sweeps only an existing `<home>/tmp`: when no cache dir was ever created
/// (e.g. `storage.mode=memory` on a fresh home) there is nothing to sweep and
/// nothing is written.
pub fn run_if_due(home: &Path) -> bool {
    let tmp = home.join("tmp");
    if !tmp.is_dir() {
        return false;
    }
    let marker = tmp.join(MARKER);
    if let Ok(text) = fs::read_to_string(&marker)
        && let Ok(epoch) = text.trim().parse::<u64>()
    {
        let last = SystemTime::UNIX_EPOCH + Duration::from_secs(epoch);
        if last.elapsed().is_ok_and(|elapsed| elapsed < INTERVAL) {
            return false;
        }
    }
    sweep_dir(&tmp.join("clone"), INTERVAL);
    sweep_dir(&tmp.join("response"), INTERVAL);
    sweep_dir(&tmp.join("tree"), INTERVAL);
    sweep_dir(&tmp.join("search-snapshots"), Duration::from_secs(60));
    // Rate-limit mirrors only hold facts that expire within ~1h.
    sweep_dir(&tmp.join("ratelimit"), INTERVAL);
    let epoch = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = fs::write(marker, epoch.to_string());
    true
}

fn sweep_dir(path: &PathBuf, max_age: Duration) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > max_age);
        if stale {
            if metadata.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_home_without_tmp_writes_nothing() {
        let home = tempfile::tempdir().unwrap();
        assert!(!run_if_due(home.path()));
        assert!(!home.path().join("tmp").exists());
    }

    #[test]
    fn existing_tmp_is_swept_once_and_writes_only_the_marker() {
        let home = tempfile::tempdir().unwrap();
        let tmp = home.path().join("tmp");
        fs::create_dir_all(&tmp).unwrap();
        assert!(run_if_due(home.path()));
        assert!(tmp.join(MARKER).is_file());
        assert!(!tmp.join("session-stats.json").exists());
        assert!(!run_if_due(home.path()));
    }
}
