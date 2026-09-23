//! Opt-in session-stats aggregation (`<home>/stats.json`).
//!
//! Records classification provider calls and provider-billed tokens for
//! session accounting. Gated on `is_stats_enabled` (persistent storage +
//! OCTOCODE_ENABLE_STATS). Recording is strictly best-effort: a stats failure
//! never fails the tool.
//!
//! Concurrency: MCP runs several tool calls at once and several processes may
//! share one home, so each update is a read-modify-write under an advisory
//! lock on a `stats.json.lock` sidecar, written through a uniquely named temp
//! file and an atomic rename. An unreadable existing file is left untouched
//! rather than reset, so a transient failure can drop one update but never
//! wipe accumulated counters.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

/// Section key under `stats` that classification usage accumulates into.
const SECTION: &str = "clasify";
const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_POLL: Duration = Duration::from_millis(10);

/// Classification usage aggregated over one tool call.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassificationUsage {
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl ClassificationUsage {
    /// Fold one provider usage record (`{input_tokens, output_tokens}`) in.
    pub fn add_record(&mut self, usage: &Value) {
        let tokens = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
        self.calls = self.calls.saturating_add(1);
        self.input_tokens = self.input_tokens.saturating_add(tokens("input_tokens"));
        self.output_tokens = self.output_tokens.saturating_add(tokens("output_tokens"));
    }
}

fn bump(entry: &mut Map<String, Value>, key: &str, by: u64) {
    if by == 0 && entry.contains_key(key) {
        return;
    }
    let current = entry.get(key).and_then(Value::as_u64).unwrap_or(0);
    entry.insert(key.to_owned(), Value::from(current.saturating_add(by)));
}

struct StatsLock(File);

impl Drop for StatsLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

fn acquire_lock(home: &Path) -> Option<StatsLock> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join("stats.json.lock"))
        .ok()?;
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Some(StatsLock(file)),
            Err(fs::TryLockError::WouldBlock) if started.elapsed() < LOCK_WAIT => {
                std::thread::sleep(LOCK_POLL);
            }
            Err(_) => return None,
        }
    }
}

fn unique_tmp(path: &Path) -> std::path::PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("json.{}.{sequence}.tmp", std::process::id()))
}

/// Record one tool call's aggregated classification usage.
pub fn record_classification(home: &Path, enabled: bool, usage: ClassificationUsage) {
    if !enabled || usage.calls == 0 {
        return;
    }
    let Some(_lock) = acquire_lock(home) else {
        return;
    };
    let path = home.join("stats.json");
    let mut root = match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) if value.is_object() => value,
            // Never reset counters because a file could not be parsed.
            _ => return,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            json!({ "version": 1, "stats": {} })
        }
        Err(_) => return,
    };
    let Some(root_map) = root.as_object_mut() else {
        return;
    };
    let stats = root_map.entry("stats").or_insert_with(|| json!({}));
    if !stats.is_object() {
        *stats = json!({});
    }
    let Some(section) = stats
        .as_object_mut()
        .map(|stats| stats.entry(SECTION).or_insert_with(|| json!({})))
    else {
        return;
    };
    if !section.is_object() {
        *section = json!({});
    }
    let Some(section) = section.as_object_mut() else {
        return;
    };
    bump(section, "calls", usage.calls);
    bump(section, "input_tokens", usage.input_tokens);
    bump(section, "output_tokens", usage.output_tokens);
    let tmp = unique_tmp(&path);
    if let Ok(serialized) = serde_json::to_string_pretty(&root)
        && fs::write(&tmp, serialized).is_ok()
        && fs::rename(&tmp, &path).is_err()
    {
        let _ = fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(calls: u64, input_tokens: u64, output_tokens: u64) -> ClassificationUsage {
        ClassificationUsage {
            calls,
            input_tokens,
            output_tokens,
        }
    }

    fn read(dir: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(dir.join("stats.json")).unwrap()).unwrap()
    }

    #[test]
    fn classification_usage_is_separate_and_has_no_workflow_counters() {
        let dir = tempfile::tempdir().unwrap();
        record_classification(dir.path(), false, usage(1, 10, 2));
        assert!(!dir.path().join("stats.json").exists());
        record_classification(dir.path(), true, usage(1, 10, 2));
        let stats = read(dir.path());
        assert_eq!(
            stats["stats"]["clasify"],
            json!({"calls": 1, "input_tokens": 10, "output_tokens": 2})
        );
        assert!(stats["stats"]["clasify"].get("gates_skipped").is_none());
    }

    #[test]
    fn records_fold_into_one_aggregate() {
        let mut total = ClassificationUsage::default();
        total.add_record(&json!({"input_tokens": 3, "output_tokens": 1}));
        total.add_record(&json!({"input_tokens": 4}));
        total.add_record(&Value::Null);
        assert_eq!(total, usage(3, 7, 1));
    }

    #[test]
    fn preserves_existing_stats_and_tolerates_legacy_shapes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("stats.json"),
            r#"{"version":1,"stats":{"toolCalls":4,"clasify":"legacy"}}"#,
        )
        .unwrap();
        record_classification(dir.path(), true, usage(1, 9, 1));
        let stats = read(dir.path());
        assert_eq!(stats["stats"]["toolCalls"], 4);
        assert_eq!(stats["stats"]["clasify"]["calls"], 1);
        assert_eq!(stats["stats"]["clasify"]["input_tokens"], 9);
    }

    #[test]
    fn unparseable_existing_file_is_never_reset() {
        let dir = tempfile::tempdir().unwrap();
        let partial = r#"{"version":1,"stats":{"clasify":{"calls":41"#;
        fs::write(dir.path().join("stats.json"), partial).unwrap();
        record_classification(dir.path(), true, usage(1, 1, 1));
        assert_eq!(
            fs::read_to_string(dir.path().join("stats.json")).unwrap(),
            partial
        );
    }

    #[test]
    fn empty_usage_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        record_classification(dir.path(), true, ClassificationUsage::default());
        assert!(!dir.path().join("stats.json").exists());
    }

    #[test]
    fn concurrent_recorders_never_lose_updates() {
        let dir = tempfile::tempdir().unwrap();
        let threads = 8;
        let per_thread = 25;
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    for _ in 0..per_thread {
                        record_classification(dir.path(), true, usage(1, 2, 1));
                    }
                });
            }
        });
        let total = (threads * per_thread) as u64;
        assert_eq!(
            read(dir.path())["stats"]["clasify"],
            json!({"calls": total, "input_tokens": total * 2, "output_tokens": total})
        );
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }
}
