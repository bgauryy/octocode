//! Language-server prewarm (`lsp.prewarm`).
//!
//! A local read or search names a source file before an agent asks for its
//! identity. When enabled, that file's server is started in the shared pool
//! in the background, under exactly the configuration `lspSearch` would
//! resolve for the same file (same workspace root, same discovery options,
//! no `rustContext` overlay, so rust-analyzer keeps its no-build-scripts /
//! no-proc-macros defaults), and the file is opened so a project load starts
//! too. The later `lspSearch` then acquires the warm pooled client.
//!
//! Bounded: at most [`MAX_PREWARM_KEYS`] simultaneous starts per runtime,
//! one in-flight start per server, and nothing is awaited by the caller.

use super::LspExecutionConfig;
use crate::policy::path::PathPolicy;
use octocode_engine::lsp::config::default_server_for_file;
use octocode_engine::lsp::pool::{LspClientPool, canonical_lsp_key};
use octocode_engine::lsp::read_regular_bounded;
use octocode_engine::lsp::types::JsLanguageServerConfig;
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Simultaneous starts and recently completed servers per runtime.
const MAX_PREWARM_KEYS: usize = 3;
/// A pooled server may have exited or been evicted; retry its prewarm after
/// this interval instead of treating a former success as permanent.
const PREWARM_COOLDOWN: Duration = Duration::from_secs(30);
/// Bound on the background open's project-load wait.
const PREWARM_READY_TIMEOUT_MS: u32 = 20_000;
const PREWARM_SETTLE_MS: u32 = 400;
/// Largest file the prewarm opens (the anchor read applies the real bound).
const MAX_PREWARM_OPEN_BYTES: u64 = 2 * 1024 * 1024;
/// Readiness string of a wait that ended with the server still indexing.
const TIMEOUT: &str = "timeout";

/// The source file a local tool row points at: the top search hit, else the
/// row's or query's own `path` when it is a file. Relative paths are not guessed.
#[must_use]
pub fn anchor_file(query: &Value, data: &Value) -> Option<PathBuf> {
    let hit = data
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .and_then(|file| file.get("path"))
        .and_then(Value::as_str);
    hit.into_iter()
        .chain(data.get("path").and_then(Value::as_str))
        .chain(query.get("path").and_then(Value::as_str))
        .map(PathBuf::from)
        .find(|path| path.is_absolute() && path.is_file())
}

/// The anchor file of the first `lspSearch` call a tool row offers (a lead
/// or page `{tool, query}` anywhere in `data`), when its `path` names an
/// absolute file path or `file://` uri of an existing file.
#[must_use]
pub fn lead_file(data: &Value) -> Option<PathBuf> {
    fn find(value: &Value, depth: usize) -> Option<PathBuf> {
        if depth > 8 {
            return None;
        }
        match value {
            Value::Object(map) => {
                if map.get("tool").and_then(Value::as_str) == Some("lspSearch") {
                    let path = crate::tools::result::continuation_row(value)?
                        .get("path")?
                        .as_str()?;
                    let path = PathBuf::from(path.strip_prefix("file://").unwrap_or(path));
                    return (path.is_absolute() && path.is_file()).then_some(path);
                }
                map.values().find_map(|child| find(child, depth + 1))
            }
            Value::Array(items) => items.iter().find_map(|item| find(item, depth + 1)),
            _ => None,
        }
    }
    find(data, 0)
}

#[derive(Debug, Default)]
pub(crate) struct WarmState {
    in_flight: HashSet<String>,
    recent: VecDeque<(String, Instant)>,
}

impl WarmState {
    fn begin(&mut self, key: &str) -> bool {
        self.recent
            .retain(|(_, warmed)| warmed.elapsed() < PREWARM_COOLDOWN);
        if self.in_flight.len() >= MAX_PREWARM_KEYS
            || self.in_flight.contains(key)
            || self.recent.iter().any(|(recent, _)| recent == key)
        {
            return false;
        }
        self.in_flight.insert(key.to_owned());
        true
    }

    fn finish(&mut self, key: &str, success: bool) {
        self.in_flight.remove(key);
        if success {
            self.recent.push_back((key.to_owned(), Instant::now()));
            if self.recent.len() > MAX_PREWARM_KEYS {
                self.recent.pop_front();
            }
        }
    }
}

/// Start `file`'s language server in the background when it is authorized,
/// has a configured server, and its key was not prewarmed yet. Returns
/// whether a start was scheduled.
pub fn schedule(
    handle: &tokio::runtime::Handle,
    pool: &Arc<LspClientPool>,
    paths: &PathPolicy,
    execution: &LspExecutionConfig,
    file: &Path,
) -> bool {
    let Ok(file) = paths.validate_read(file) else {
        return false;
    };
    let file = file.canonical.to_string_lossy().into_owned();
    let Some(workspace) = resolve_workspace_root_for_file(file.clone())
        .ok()
        .and_then(|root| paths.validate(&root).ok())
        .filter(|root| root.canonical.is_dir())
        .map(|root| root.canonical.to_string_lossy().into_owned())
    else {
        return false;
    };
    let config_path = match execution.config_path.as_deref() {
        Some(path) => match paths.validate_read(path) {
            Ok(validated) => Some(validated.canonical),
            Err(_) => return false,
        },
        None => None,
    };
    let discovery = execution.discovery(config_path);
    let Some(config) = default_server_for_file(&file, &workspace, &discovery) else {
        return false;
    };
    let Ok(key) = canonical_lsp_key(&config) else {
        return false;
    };
    {
        let Ok(mut state) = execution.prewarm_state.lock() else {
            return false;
        };
        if !state.begin(&key) {
            return false;
        }
    }
    let pool = pool.clone();
    let state = execution.prewarm_state.clone();
    handle.spawn(async move {
        let success = warm(&pool, config, file).await;
        if let Ok(mut state) = state.lock() {
            state.finish(&key, success);
        }
    });
    true
}

/// Start (or reuse) `config`'s pooled server and open `file` so its project
/// load starts too. `true` only when the server is ready and the file was
/// opened without a readiness timeout.
pub(super) async fn warm(
    pool: &LspClientPool,
    config: JsLanguageServerConfig,
    file: String,
) -> bool {
    let warmed = async {
        let client = pool.acquire(config).await.ok().flatten()?;
        // Still indexing at its readiness budget: not warm yet. The pooled
        // server keeps indexing, and its next acquire waits again.
        if client.readiness().as_deref() == Some(TIMEOUT) {
            return None;
        }
        // One bounded read of a regular file. A file over the open cap (or
        // unreadable) warmed only the server.
        let path = PathBuf::from(&file);
        let bytes = tokio::task::spawn_blocking(move || {
            read_regular_bounded(&path, MAX_PREWARM_OPEN_BYTES).ok()
        })
        .await
        .ok()??;
        let content = String::from_utf8(bytes).ok()?;
        let readiness = client
            .open_document_and_wait(
                file,
                &content,
                Some(PREWARM_SETTLE_MS),
                Some(PREWARM_READY_TIMEOUT_MS),
            )
            .await
            .ok()?;
        (readiness.as_deref() != Some(TIMEOUT)).then_some(())
    };
    warmed.await.is_some()
}

#[cfg(test)]
mod tests {
    use super::{WarmState, anchor_file, lead_file};
    use serde_json::json;

    #[test]
    fn completed_prewarms_do_not_exhaust_future_workspaces() {
        let mut state = WarmState::default();
        for i in 0..3 {
            let key = format!("workspace-{i}");
            assert!(state.begin(&key));
            state.finish(&key, true);
        }
        assert!(state.begin("workspace-3"));
        assert!(!state.begin("workspace-3"));
    }

    #[test]
    fn a_previously_warm_server_can_be_warmed_again_after_cooldown() {
        let mut state = WarmState::default();
        assert!(state.begin("workspace"));
        state.finish("workspace", true);
        assert!(!state.begin("workspace"));
        state.recent.front_mut().unwrap().1 =
            std::time::Instant::now() - super::PREWARM_COOLDOWN - std::time::Duration::from_secs(1);
        assert!(state.begin("workspace"));
    }

    #[test]
    fn anchor_prefers_the_top_hit_then_a_file_query_path() {
        let dir = std::env::temp_dir().join(format!("octocode-prewarm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let hit = dir.join("hit.ts");
        let read = dir.join("read.ts");
        std::fs::write(&hit, "export const a = 1;\n").expect("hit");
        std::fs::write(&read, "export const b = 1;\n").expect("read");
        let search = json!({"path": dir});
        let data = json!({"files": [{"path": hit}, {"path": read}]});
        assert_eq!(anchor_file(&search, &data), Some(hit.clone()));
        assert_eq!(anchor_file(&json!({"path": read}), &json!({})), Some(read));
        // A directory query with no hits, or relative paths, name no file.
        assert_eq!(anchor_file(&search, &json!({"files": []})), None);
        assert_eq!(
            anchor_file(
                &json!({"path": "hit.ts"}),
                &json!({"files": [{"path": "hit.ts"}]})
            ),
            None
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_lead_naming_lsp_search_points_at_its_anchor_file() {
        let dir =
            std::env::temp_dir().join(format!("octocode-prewarm-lead-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("a.ts");
        std::fs::write(&file, "export const a = 1;\n").expect("file");
        let uri = format!("file://{}", file.display());
        let data = json!({"results":[{"file":"a.ts"}],"next":{"references":{"tool":"lspSearch","query":{"queries":[{"path":uri,"symbolName":"a","lineHint":1}]}}}});
        assert_eq!(lead_file(&data), Some(file.clone()));
        let other =
            json!({"next":{"read":{"tool":"localFetch","query":{"queries":[{"path":file}]}}}});
        assert_eq!(lead_file(&other), None);
        let relative =
            json!({"hints":{"refs":{"tool":"lspSearch","query":{"queries":[{"path":"a.ts"}]}}}});
        assert_eq!(lead_file(&relative), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
