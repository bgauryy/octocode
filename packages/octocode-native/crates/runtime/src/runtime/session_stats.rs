//! Opt-in session-stats aggregation (`<home>/stats.json`).
//!
//! Records Jev calls and provider-billed tokens for session accounting.
//! Gated on `is_stats_enabled` (persistent storage + OCTOCODE_ENABLE_STATS).
//! Recording is strictly best-effort: a stats failure never fails the tool.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

fn bump(entry: &mut Map<String, Value>, key: &str, by: u64) {
    if by == 0 && entry.contains_key(key) {
        return;
    }
    let current = entry.get(key).and_then(Value::as_u64).unwrap_or(0);
    entry.insert(key.to_owned(), Value::from(current.saturating_add(by)));
}

/// Record one pure Jev evaluation without adding workflow metadata.
pub fn record_jev(home: &Path, enabled: bool, payload: &Value) {
    if !enabled {
        return;
    }
    let Some(usage) = payload.get("usage") else {
        return;
    };
    let input = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let path = home.join("stats.json");
    let mut root: Value = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "version": 1, "stats": {} }));
    let Some(root_map) = root.as_object_mut() else {
        return;
    };
    let stats = root_map.entry("stats").or_insert_with(|| json!({}));
    if !stats.is_object() {
        *stats = json!({});
    }
    // Both values were just normalized to `{}` above when not already objects.
    #[allow(clippy::expect_used)]
    let jev = stats
        .as_object_mut()
        .expect("stats normalized to object")
        .entry("semanticAssess")
        .or_insert_with(|| json!({}));
    if !jev.is_object() {
        *jev = json!({});
    }
    #[allow(clippy::expect_used)]
    let jev_map = jev.as_object_mut().expect("jev normalized to object");
    bump(jev_map, "calls", 1);
    bump(jev_map, "input_tokens", input);
    bump(jev_map, "output_tokens", output);
    let tmp = path.with_extension("json.tmp");
    if let Ok(serialized) = serde_json::to_string_pretty(&root)
        && fs::write(&tmp, serialized).is_ok()
    {
        let _ = fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_jev_usage_is_separate_and_has_no_workflow_counters() {
        let dir = tempfile::tempdir().unwrap();
        let payload = json!({"model": "jev-test", "answer": {"type":"noul","noul":0.8}, "usage": {"input_tokens": 10, "output_tokens": 2}});
        record_jev(dir.path(), false, &payload);
        assert!(!dir.path().join("stats.json").exists());
        record_jev(dir.path(), true, &payload);
        let stats: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join("stats.json")).unwrap())
                .unwrap();
        assert_eq!(
            stats["stats"]["semanticAssess"],
            json!({"calls": 1, "input_tokens": 10, "output_tokens": 2})
        );
        assert!(
            stats["stats"]["semanticAssess"]
                .get("gates_skipped")
                .is_none()
        );
    }

    #[test]
    fn preserves_existing_stats_and_tolerates_legacy_shapes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("stats.json"),
            r#"{"version":1,"stats":{"toolCalls":4,"jev":"legacy"}}"#,
        )
        .unwrap();
        record_jev(
            dir.path(),
            true,
            &json!({"usage":{"input_tokens":9,"output_tokens":1}}),
        );
        let stats: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join("stats.json")).unwrap())
                .unwrap();
        assert_eq!(stats["stats"]["toolCalls"], 4);
        assert_eq!(stats["stats"]["semanticAssess"]["calls"], 1);
        assert_eq!(stats["stats"]["semanticAssess"]["input_tokens"], 9);
    }

    #[test]
    fn payloads_without_usage_write_nothing() {
        let dir = tempfile::tempdir().unwrap();
        record_jev(dir.path(), true, &json!({"unrelated":true}));
        assert!(!dir.path().join("stats.json").exists());
    }
}
