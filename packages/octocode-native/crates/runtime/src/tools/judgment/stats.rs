//! Opt-in session-stats aggregation (`<home>/stats.json`).
//!
//! Records successful classification provider responses and reported tokens for
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

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::Duration;

use serde_json::{Map, Value, json};

/// Section key under `stats` that classification usage accumulates into.
const SECTION: &str = crate::id::ToolId::Clasify.as_str();
const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_POLL: Duration = Duration::from_millis(10);

/// Classification usage aggregated over one tool call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ClassificationUsage {
    pub calls: u64,
    pub known_usage_calls: u64,
    pub unknown_usage_calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub(crate) discovery_calls: u64,
    pub(crate) replayed_evaluations: u64,
    pub models: BTreeSet<String>,
}

impl ClassificationUsage {
    pub(crate) fn output(&self) -> octocode_contract_types::SelectionUsage {
        let count = |value| i64::try_from(value).unwrap_or(i64::MAX);
        octocode_contract_types::SelectionUsage {
            calls: count(self.calls),
            known_usage_calls: count(self.known_usage_calls),
            unknown_usage_calls: count(self.unknown_usage_calls),
            input_tokens: count(self.input_tokens),
            output_tokens: count(self.output_tokens),
            replayed_evaluations: count(self.replayed_evaluations),
            models: self
                .models
                .iter()
                .filter_map(|model| model.parse().ok())
                .collect(),
        }
    }

    /// Fold in `calls` provider requests, `known` of which reported their
    /// tokens. Retained directional totals are never called complete when
    /// a request did not report them; a reported zero is known usage.
    pub(crate) fn add_usage(
        &mut self,
        calls: u64,
        known: u64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        let known = known.min(calls);
        self.calls = self.calls.saturating_add(calls);
        self.known_usage_calls = self.known_usage_calls.saturating_add(known);
        self.unknown_usage_calls = self.unknown_usage_calls.saturating_add(calls - known);
        self.input_tokens = self.input_tokens.saturating_add(input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(output_tokens);
    }

    pub(crate) fn merge(&mut self, other: &Self) {
        self.calls = self.calls.saturating_add(other.calls);
        self.known_usage_calls = self
            .known_usage_calls
            .saturating_add(other.known_usage_calls);
        self.unknown_usage_calls = self
            .unknown_usage_calls
            .saturating_add(other.unknown_usage_calls);
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.discovery_calls = self.discovery_calls.saturating_add(other.discovery_calls);
        self.replayed_evaluations = self
            .replayed_evaluations
            .saturating_add(other.replayed_evaluations);
        self.models.extend(other.models.iter().cloned());
    }

    /// Fold one evaluation in: its provider requests and reported tokens. A
    /// replay made no request; its tokens are the earlier evaluation's.
    pub(crate) fn add(&mut self, outcome: &Result<super::Judged, super::ClassificationError>) {
        match outcome {
            Ok(judged) => {
                self.models.insert(judged.model.clone());
                if judged.replayed {
                    self.replayed_evaluations = self.replayed_evaluations.saturating_add(1);
                }
                if judged.provider_calls > 0 {
                    self.add_usage(
                        judged.provider_calls,
                        judged.known_usage_calls,
                        judged.input_tokens,
                        judged.output_tokens,
                    );
                }
            }
            Err(error) if error.provider_calls > 0 => {
                let usage = error.usage();
                self.add_usage(
                    error.provider_calls,
                    usage.known_usage_calls,
                    usage.input_tokens,
                    usage.output_tokens,
                );
            }
            Err(_) => {}
        }
    }
}

fn bump(entry: &mut Map<String, Value>, key: &str, by: u64) {
    if by == 0 && entry.contains_key(key) {
        return;
    }
    let current = entry.get(key).and_then(Value::as_u64).unwrap_or(0);
    entry.insert(key.to_owned(), Value::from(current.saturating_add(by)));
}

fn acquire_lock(home: &Path) -> Option<octocode_io::lock::LockedFile> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join("stats.json.lock"))
        .ok()?;
    octocode_io::lock::lock_within(file, LOCK_WAIT, LOCK_POLL).ok()
}

/// Record one tool call's aggregated classification usage.
pub(crate) fn record_classification(home: &Path, usage: ClassificationUsage) {
    if usage.calls == 0 && usage.discovery_calls == 0 {
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
    if usage.discovery_calls > 0 {
        bump(section, "discovery_calls", usage.discovery_calls);
    }
    bump(section, "calls", usage.calls);
    bump(section, "known_usage_calls", usage.known_usage_calls);
    bump(section, "unknown_usage_calls", usage.unknown_usage_calls);
    bump(section, "input_tokens", usage.input_tokens);
    bump(section, "output_tokens", usage.output_tokens);
    if let Ok(serialized) = serde_json::to_string_pretty(&root) {
        let _ = octocode_io::atomic::write_atomic(&path, serialized.as_bytes(), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(calls: u64, input_tokens: u64, output_tokens: u64) -> ClassificationUsage {
        ClassificationUsage {
            calls,
            known_usage_calls: calls,
            unknown_usage_calls: 0,
            input_tokens,
            output_tokens,
            ..ClassificationUsage::default()
        }
    }

    fn read(dir: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(dir.join("stats.json")).unwrap()).unwrap()
    }

    #[test]
    fn classification_usage_is_separate_and_has_no_workflow_counters() {
        let dir = tempfile::tempdir().unwrap();
        record_classification(dir.path(), usage(1, 10, 2));
        let stats = read(dir.path());
        assert_eq!(
            stats["stats"]["clasify"],
            json!({"calls": 1, "known_usage_calls": 1, "unknown_usage_calls": 0, "input_tokens": 10, "output_tokens": 2})
        );
        assert!(stats["stats"]["clasify"].get("gates_skipped").is_none());
    }

    #[test]
    fn requests_fold_into_one_aggregate_and_unreported_usage_stays_unknown() {
        let mut total = ClassificationUsage::default();
        total.add_usage(2, 1, 10, 3);
        total.add_usage(1, 0, 0, 0);
        total.add_usage(1, 1, 0, 0);
        total.add_usage(1, 5, 4, 1);
        assert_eq!(total.calls, 5);
        assert_eq!(total.known_usage_calls, 3, "known never exceeds calls");
        assert_eq!(total.unknown_usage_calls, 2);
        assert_eq!(total.input_tokens, 14);
        assert_eq!(total.output_tokens, 4);
    }

    #[test]
    fn a_file_from_an_older_build_is_not_reinterpreted() {
        // An older build wrote `calls` without usage counters; the new call adds
        // only its own usage, and no historical calls are inferred as unknown.
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("stats.json"),
            r#"{"version":1,"stats":{"clasify":{"calls":5,"input_tokens":50,"output_tokens":5}}}"#,
        )
        .unwrap();
        record_classification(dir.path(), usage(1, 9, 1));
        assert_eq!(
            read(dir.path())["stats"]["clasify"],
            json!({"calls": 6, "known_usage_calls": 1, "unknown_usage_calls": 0, "input_tokens": 59, "output_tokens": 6})
        );
    }

    #[test]
    fn tolerates_unexpected_stats_shapes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("stats.json"),
            r#"{"version":1,"stats":{"toolCalls":4,"clasify":"legacy"}}"#,
        )
        .unwrap();
        record_classification(dir.path(), usage(1, 9, 1));
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
        record_classification(dir.path(), usage(1, 1, 1));
        assert_eq!(
            fs::read_to_string(dir.path().join("stats.json")).unwrap(),
            partial
        );
    }

    #[test]
    fn empty_usage_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        record_classification(dir.path(), ClassificationUsage::default());
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
                        record_classification(dir.path(), usage(1, 2, 1));
                    }
                });
            }
        });
        let total = (threads * per_thread) as u64;
        assert_eq!(
            read(dir.path())["stats"]["clasify"],
            json!({"calls": total, "known_usage_calls": total, "unknown_usage_calls": 0, "input_tokens": total * 2, "output_tokens": total})
        );
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }
}
