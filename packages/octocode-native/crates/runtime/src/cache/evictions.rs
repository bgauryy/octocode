//! Append-only eviction trail for on-disk caches.
//!
//! Every clone-cache removal (ghCloneRepo) appends one JSON line to
//! `<octocode home>/logs/evictions.jsonl` so a vanished checkout is always
//! attributable (reason, path, bytes, pid, timestamp). Writes are strictly
//! best-effort: a log failure must never block or fail the eviction itself.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_DIR: &str = "logs";
const LOG_FILE: &str = "evictions.jsonl";
/// Rotate to `evictions.jsonl.1` once the live file exceeds this size.
const ROTATE_BYTES: u64 = 1_000_000;

/// Append one eviction record. Never fails, never panics.
pub fn log_eviction(home: &Path, reason: &str, path: &Path, bytes: u64) {
    let _ = try_log(home, reason, path, bytes);
}

fn try_log(home: &Path, reason: &str, path: &Path, bytes: u64) -> std::io::Result<()> {
    let dir = home.join(LOG_DIR);
    fs::create_dir_all(&dir)?;
    let file = dir.join(LOG_FILE);
    if fs::metadata(&file)
        .map(|m| m.len() >= ROTATE_BYTES)
        .unwrap_or(false)
    {
        let _ = fs::rename(&file, dir.join(format!("{LOG_FILE}.1")));
    }
    let line = serde_json::json!({
        "at": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        "reason": reason,
        "path": path.to_string_lossy(),
        "bytes": bytes,
        "pid": std::process::id(),
    });
    let mut handle = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)?;
    writeln!(handle, "{line}")
}

/// Last `limit` eviction lines (oldest first), plus the live log size in
/// bytes. Best-effort: an unreadable log reads as empty.
pub fn recent_evictions(home: &Path, limit: usize) -> (Vec<String>, u64) {
    let file = home.join(LOG_DIR).join(LOG_FILE);
    let size = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let Ok(text) = fs::read_to_string(&file) else {
        return (vec![], size);
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(limit);
    (
        lines[start..].iter().map(|s| (*s).to_owned()).collect(),
        size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_shaped_lines_and_tails_in_order() {
        let root = tempfile::tempdir().expect("test fixture operation should succeed");
        let home = root.path();
        log_eviction(home, "ttl", Path::new("/cache/a"), 10);
        log_eviction(home, "size-limit", Path::new("/cache/b"), 20);
        let (lines, size) = recent_evictions(home, 5);
        assert_eq!(lines.len(), 2);
        assert!(size > 0);
        let first: serde_json::Value =
            serde_json::from_str(&lines[0]).expect("test fixture operation should succeed");
        assert_eq!(first["reason"], "ttl");
        assert_eq!(first["path"], "/cache/a");
        assert_eq!(first["bytes"], 10);
        assert_eq!(first["pid"], std::process::id());
        assert!(first["at"].as_u64().is_some());
        let (tail_one, _) = recent_evictions(home, 1);
        assert!(tail_one[0].contains("size-limit"));
    }

    #[test]
    fn rotates_at_size_cap_and_missing_log_reads_empty() {
        let root = tempfile::tempdir().expect("test fixture operation should succeed");
        let home = root.path();
        let (lines, size) = recent_evictions(home, 5);
        assert!(lines.is_empty());
        assert_eq!(size, 0);
        let dir = home.join(LOG_DIR);
        fs::create_dir_all(&dir).expect("test fixture operation should succeed");
        fs::write(dir.join(LOG_FILE), vec![b'x'; ROTATE_BYTES as usize])
            .expect("test fixture operation should succeed");
        log_eviction(home, "ttl", Path::new("/cache/rotated"), 1);
        assert!(dir.join(format!("{LOG_FILE}.1")).is_file());
        let (lines, size) = recent_evictions(home, 5);
        assert_eq!(lines.len(), 1, "live log restarts after rotation");
        assert!(size < ROTATE_BYTES);
    }

    #[test]
    fn unwritable_log_never_blocks() {
        // A file where the logs directory should be makes create_dir_all fail.
        let root = tempfile::tempdir().expect("test fixture operation should succeed");
        let home = root.path();
        fs::write(home.join(LOG_DIR), b"not a directory")
            .expect("test fixture operation should succeed");
        log_eviction(home, "ttl", Path::new("/cache/x"), 0);
    }
}
