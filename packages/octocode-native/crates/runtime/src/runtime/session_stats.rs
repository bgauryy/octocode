//! Opt-in session-stats aggregation (`<home>/stats.json`).
//!
//! Records jevReasoning usage so a host can account classic-LLM plus jev-LLM
//! consumption across a session: judgment calls with provider-billed tokens,
//! deterministic gate short-circuits (calls avoided), and blocked applications.
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

/// Record one jevReasoning per-query payload into `<home>/stats.json`.
pub fn record_jev(home: &Path, enabled: bool, payload: &Value) {
    if !enabled {
        return;
    }
    let gate = payload.get("gate").and_then(Value::as_str);
    if gate.is_none() {
        return;
    }
    let usage = payload.get("usage");
    let judgment = gate == Some("judgment") && usage.is_some();
    let input = usage
        .and_then(|u| u.get("input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output = usage
        .and_then(|u| u.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let blocked = payload
        .get("applied")
        .and_then(|a| a.get("blocked"))
        .and_then(Value::as_bool)
        == Some(true);

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
    let jev = stats
        .as_object_mut()
        .expect("stats normalized to object")
        .entry("jevReasoning")
        .or_insert_with(|| json!({}));
    if !jev.is_object() {
        *jev = json!({});
    }
    let jev_map = jev.as_object_mut().expect("jev normalized to object");
    bump(jev_map, "calls", u64::from(judgment));
    bump(jev_map, "input_tokens", input);
    bump(jev_map, "output_tokens", output);
    bump(jev_map, "gates_skipped", u64::from(!judgment));
    bump(jev_map, "blocked", u64::from(blocked));

    let tmp = path.with_extension("json.tmp");
    if let Ok(serialized) = serde_json::to_string_pretty(&root)
        && fs::write(&tmp, serialized).is_ok()
    {
        let _ = fs::rename(&tmp, &path);
    }
}

/// Record one jevScout per-query payload into `<home>/stats.json`.
/// Aggregates calls, billed tokens, and the action split so classic+jev
/// accounting can attribute avoided reads (skips) per session.
pub fn record_scout(home: &Path, enabled: bool, payload: &Value) {
    if !enabled {
        return;
    }
    let Some(results) = payload.get("results").and_then(Value::as_object) else {
        return;
    };
    let count = |action: &str| {
        results
            .values()
            .filter(|row| row.get("action").and_then(Value::as_str) == Some(action))
            .count() as u64
    };
    let usage = payload.get("usage");
    let token = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };

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
    let scout = stats
        .as_object_mut()
        .expect("stats normalized to object")
        .entry("jevScout")
        .or_insert_with(|| json!({}));
    if !scout.is_object() {
        *scout = json!({});
    }
    let scout_map = scout.as_object_mut().expect("scout normalized to object");
    bump(scout_map, "calls", 1);
    bump(scout_map, "input_tokens", token("input_tokens"));
    bump(scout_map, "output_tokens", token("output_tokens"));
    bump(scout_map, "reads", count("read"));
    bump(scout_map, "skips", count("skip"));
    bump(scout_map, "gray_reads", count("gray_read"));

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

    fn payload(gate: &str, usage: Option<(u64, u64)>, blocked: bool) -> Value {
        let mut value = json!({ "gate": gate, "applied": { "blocked": blocked } });
        if let Some((input, output)) = usage {
            value["usage"] = json!({ "input_tokens": input, "output_tokens": output });
        }
        value
    }

    #[test]
    fn records_judgments_gates_and_blocks() {
        let dir = tempfile::tempdir().expect("tempdir");
        record_jev(
            dir.path(),
            true,
            &payload("judgment", Some((100, 7)), false),
        );
        record_jev(dir.path(), true, &payload("judgment", Some((50, 3)), true));
        record_jev(dir.path(), true, &payload("deterministic", None, false));
        let stats: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("stats.json")).expect("stats written"),
        )
        .expect("valid json");
        let jev = &stats["stats"]["jevReasoning"];
        assert_eq!(jev["calls"], 2);
        assert_eq!(jev["input_tokens"], 150);
        assert_eq!(jev["output_tokens"], 10);
        assert_eq!(jev["gates_skipped"], 1);
        assert_eq!(jev["blocked"], 1);
    }

    #[test]
    fn disabled_or_gateless_payloads_write_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        record_jev(dir.path(), false, &payload("judgment", Some((1, 1)), false));
        record_jev(dir.path(), true, &json!({ "unrelated": true }));
        assert!(!dir.path().join("stats.json").exists());
    }

    #[test]
    fn records_scout_action_split_and_usage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let scout = json!({
            "status": "scouted",
            "results": {
                "a.rs": { "action": "read" },
                "b.rs": { "action": "skip" },
                "c.rs": { "action": "skip" },
                "d.rs": { "action": "gray_read" }
            },
            "usage": { "input_tokens": 900, "output_tokens": 40 }
        });
        record_scout(dir.path(), true, &scout);
        record_scout(dir.path(), true, &scout);
        record_scout(dir.path(), false, &scout); // disabled: ignored
        let stats: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("stats.json")).expect("stats written"),
        )
        .expect("valid json");
        let scout_stats = &stats["stats"]["jevScout"];
        assert_eq!(scout_stats["calls"], 2);
        assert_eq!(scout_stats["input_tokens"], 1800);
        assert_eq!(scout_stats["reads"], 2);
        assert_eq!(scout_stats["skips"], 4);
        assert_eq!(scout_stats["gray_reads"], 2);
        // resultless payloads write nothing new
        record_scout(dir.path(), true, &json!({ "unrelated": true }));
        let again: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("stats.json")).expect("stats"),
        )
        .expect("valid json");
        assert_eq!(again["stats"]["jevScout"]["calls"], 2);
    }

    #[test]
    fn preserves_existing_stats_and_tolerates_legacy_shapes() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("stats.json"),
            r#"{ "version": 1, "stats": { "toolCalls": 4, "jevReasoning": "legacy" } }"#,
        )
        .expect("seed");
        record_jev(dir.path(), true, &payload("judgment", Some((9, 1)), false));
        let stats: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("stats.json")).expect("stats"),
        )
        .expect("valid json");
        assert_eq!(stats["stats"]["toolCalls"], 4);
        assert_eq!(stats["stats"]["jevReasoning"]["calls"], 1);
        assert_eq!(stats["stats"]["jevReasoning"]["input_tokens"], 9);
    }
}
