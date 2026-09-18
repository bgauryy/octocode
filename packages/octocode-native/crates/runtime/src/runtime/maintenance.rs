//! Shared 24-hour sweep of native tmp caches.
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MARKER: &str = ".last-cache-maintenance";

pub fn run_if_due(home: &Path) -> bool {
    let tmp = home.join("tmp");
    let marker = tmp.join(MARKER);
    if let Ok(text) = fs::read_to_string(&marker)
        && let Ok(epoch) = text.trim().parse::<u64>()
    {
        let last = SystemTime::UNIX_EPOCH + Duration::from_secs(epoch);
        if last.elapsed().is_ok_and(|elapsed| elapsed < INTERVAL) {
            return false;
        }
    }
    let _ = fs::create_dir_all(&tmp);
    sweep_dir(&tmp.join("clone"), INTERVAL);
    sweep_dir(&tmp.join("response"), INTERVAL);
    sweep_dir(&tmp.join("tree"), INTERVAL);
    sweep_dir(&tmp.join("search-snapshots"), Duration::from_secs(60));
    let epoch = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let _ = fs::write(marker, epoch.to_string());
    let stats = crate::providers::github::session_snapshot();
    let _ = fs::write(tmp.join("session-stats.json"), stats.to_string());
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
