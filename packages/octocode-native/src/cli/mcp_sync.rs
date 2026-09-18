//! Read-only MCP config sync analysis across JSON clients: which servers are
//! present, missing, or conflicting between the detected client configs.
//!
//! Mirrors the TypeScript `status --sync` analysis (`features/sync.ts`
//! `readAllClientConfigs` + `analyzeSyncState` + `areMCPServersEqual`). The dead
//! `quickSync` writer path is intentionally not ported \u2014 it has no callers.
use super::mcp_install::{config_path, json_clients};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// One client's MCP server table (JSON clients only).
pub struct ClientSnapshot {
    pub client: &'static str,
    pub exists: bool,
    pub servers: Map<String, Value>,
}

/// Read every detectable JSON client's `mcpServers` table from disk.
pub fn read_all_client_configs() -> Vec<ClientSnapshot> {
    let mut snapshots = Vec::new();
    for client in json_clients() {
        let Some(path) = config_path(client) else {
            continue;
        };
        let exists = path.exists();
        let servers = if exists {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .and_then(|value| value.get("mcpServers").and_then(Value::as_object).cloned())
                .unwrap_or_default()
        } else {
            Map::new()
        };
        snapshots.push(ClientSnapshot {
            client,
            exists,
            servers,
        });
    }
    snapshots
}

/// A server entry's `args` array, treating a missing array as empty.
fn args_slice(server: &Value) -> &[Value] {
    server
        .get("args")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Structural equality of two MCP server entries: command, ordered args, and
/// unordered env map. Missing `args`/`env` normalize to empty.
pub fn servers_equal(a: &Value, b: &Value) -> bool {
    if a.get("command") != b.get("command") {
        return false;
    }
    if args_slice(a) != args_slice(b) {
        return false;
    }
    let env_a: BTreeMap<&String, &Value> = a
        .get("env")
        .and_then(Value::as_object)
        .map(|map| map.iter().collect())
        .unwrap_or_default();
    let env_b: BTreeMap<&String, &Value> = b
        .get("env")
        .and_then(Value::as_object)
        .map(|map| map.iter().collect())
        .unwrap_or_default();
    env_a == env_b
}

pub struct SyncSummary {
    pub total_unique_mcps: usize,
    pub consistent: usize,
    pub needs_sync: usize,
    pub conflicts: usize,
    pub clients_with_config: usize,
}

pub struct SyncAnalysis {
    pub summary: SyncSummary,
    /// (mcpId, clients missing it) for entries that differ only by presence.
    pub needs_sync: Vec<(String, Vec<&'static str>)>,
    /// (mcpId, clients that have a conflicting variant).
    pub conflicts: Vec<(String, Vec<&'static str>)>,
}

/// Classify every unique MCP id across the detected client configs.
pub fn analyze(snapshots: &[ClientSnapshot]) -> SyncAnalysis {
    let with_config: Vec<&ClientSnapshot> = snapshots.iter().filter(|s| s.exists).collect();
    // mcpId -> ordered list of (client, server variant)
    let mut by_mcp: BTreeMap<String, Vec<(&'static str, &Value)>> = BTreeMap::new();
    for snapshot in &with_config {
        for (mcp_id, server) in &snapshot.servers {
            by_mcp
                .entry(mcp_id.clone())
                .or_default()
                .push((snapshot.client, server));
        }
    }
    let all_client_ids: Vec<&'static str> = with_config.iter().map(|s| s.client).collect();
    let total_unique = by_mcp.len();
    let mut needs_sync = Vec::new();
    let mut conflicts = Vec::new();
    let mut consistent = 0usize;
    for (mcp_id, variants) in &by_mcp {
        let present: BTreeSet<&'static str> = variants.iter().map(|(client, _)| *client).collect();
        let missing: Vec<&'static str> = all_client_ids
            .iter()
            .copied()
            .filter(|client| !present.contains(client))
            .collect();
        let mut has_conflict = false;
        if variants.len() > 1 {
            let first = variants[0].1;
            for (_, variant) in &variants[1..] {
                if !servers_equal(first, variant) {
                    has_conflict = true;
                    break;
                }
            }
        }
        if has_conflict {
            conflicts.push((
                mcp_id.clone(),
                variants.iter().map(|(client, _)| *client).collect(),
            ));
        } else if !missing.is_empty() {
            needs_sync.push((mcp_id.clone(), missing));
        } else {
            consistent += 1;
        }
    }
    SyncAnalysis {
        summary: SyncSummary {
            total_unique_mcps: total_unique,
            consistent,
            needs_sync: needs_sync.len(),
            conflicts: conflicts.len(),
            clients_with_config: with_config.len(),
        },
        needs_sync,
        conflicts,
    }
}

/// Render the analysis as the `sync` JSON block used by `status --sync`.
pub fn sync_json(analysis: &SyncAnalysis) -> Value {
    json!({
        "summary": {
            "needsSyncCount": analysis.summary.needs_sync,
            "conflictCount": analysis.summary.conflicts,
            "consistentMCPs": analysis.summary.consistent,
            "totalUniqueMCPs": analysis.summary.total_unique_mcps,
        },
        "needsSync": analysis
            .needs_sync
            .iter()
            .map(|(mcp_id, missing)| json!({ "mcpId": mcp_id, "missingIn": missing }))
            .collect::<Vec<_>>(),
        "conflicts": analysis
            .conflicts
            .iter()
            .map(|(mcp_id, present)| json!({ "mcpId": mcp_id, "presentIn": present }))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(client: &'static str, servers: Value) -> ClientSnapshot {
        ClientSnapshot {
            client,
            exists: true,
            servers: servers.as_object().cloned().unwrap_or_default(),
        }
    }

    #[test]
    fn servers_equal_normalizes_missing_args_and_env() {
        let a = json!({ "command": "npx", "args": ["-y", "octocode-mcp@latest"] });
        let b = json!({ "command": "npx", "args": ["-y", "octocode-mcp@latest"], "env": {} });
        assert!(servers_equal(&a, &b));
        let c = json!({ "command": "npx" });
        let d = json!({ "command": "npx", "args": [] });
        assert!(servers_equal(&c, &d));
        let e = json!({ "command": "npx", "args": ["a"] });
        assert!(!servers_equal(&a, &e));
        let f = json!({ "command": "bunx", "args": ["-y", "octocode-mcp@latest"] });
        assert!(!servers_equal(&a, &f));
    }

    #[test]
    fn analyze_classifies_consistent_missing_and_conflict() {
        let server = json!({ "command": "npx", "args": ["-y", "octocode-mcp@latest"] });
        let snaps = vec![
            snapshot(
                "cursor",
                json!({ "octocode": server, "other": { "command": "foo" } }),
            ),
            snapshot("zed", json!({ "octocode": server })),
            snapshot(
                "kiro",
                json!({ "octocode": { "command": "bunx", "args": [] }, "other": { "command": "foo" } }),
            ),
        ];
        let analysis = analyze(&snaps);
        // 2 unique mcps: octocode (conflict: npx vs bunx), other (missing in zed).
        assert_eq!(analysis.summary.total_unique_mcps, 2);
        assert_eq!(analysis.summary.conflicts, 1, "octocode variants differ");
        assert_eq!(analysis.summary.needs_sync, 1, "other missing in zed");
        assert_eq!(analysis.summary.consistent, 0);
        assert_eq!(analysis.summary.clients_with_config, 3);

        let conflict = &analysis.conflicts[0];
        assert_eq!(conflict.0, "octocode");
        let missing = &analysis.needs_sync[0];
        assert_eq!(missing.0, "other");
        assert_eq!(missing.1, vec!["zed"]);
    }

    #[test]
    fn analyze_reports_fully_consistent() {
        let server = json!({ "command": "npx", "args": ["-y", "octocode-mcp@latest"] });
        let snaps = vec![
            snapshot("cursor", json!({ "octocode": server })),
            snapshot("zed", json!({ "octocode": server })),
        ];
        let analysis = analyze(&snaps);
        assert_eq!(analysis.summary.consistent, 1);
        assert_eq!(analysis.summary.needs_sync, 0);
        assert_eq!(analysis.summary.conflicts, 0);
        let block = sync_json(&analysis);
        assert_eq!(block["summary"]["consistentMCPs"], 1);
    }
}
