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
//! A host that knows its workspace before any call (`octocode mcp` after
//! `initialize`) warms the workspace's servers instead
//! ([`workspace::schedule_workspace`]).
//!
//! Bounded: at most [`MAX_PREWARM_KEYS`] simultaneous starts per runtime,
//! one in-flight start per server, and nothing is awaited by the caller.

use super::LspExecutionConfig;
use crate::policy::path::PathPolicy;
use octocode_engine::lsp::config::default_server_for_file;
use octocode_engine::lsp::pool::{LspClientPool, canonical_lsp_key};
use octocode_engine::lsp::types::LspLanguageServerConfig;
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Simultaneous starts and recently completed servers per runtime.
pub(crate) const MAX_PREWARM_KEYS: usize = 3;
/// A pooled server may have exited or been evicted; retry its prewarm after
/// this interval instead of treating a former success as permanent.
const PREWARM_COOLDOWN: Duration = Duration::from_secs(30);
/// Bound on the background open's project-load wait: longer than a
/// request's own (nobody waits on it), so the load it starts can finish.
const PREWARM_READY_TIMEOUT_MS: u32 = 20_000;
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

/// The anchor of the first schema-declared lspSearch call that names an
/// existing absolute file. Provider data resembling a call is not executable.
#[must_use]
pub fn lead_file(tool: crate::id::ToolId, data: &Value) -> Option<PathBuf> {
    let structured = serde_json::json!({"results":[{"index":0,"data":data}]});
    let mut found = None;
    crate::response::continuations::visit_calls(tool, &structured, &mut |call, _| {
        if found.is_some() || call["tool"] != crate::id::ToolId::LspSearch.as_str() {
            return;
        }
        found = call["input"]["queries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|query| query["path"].as_str())
            .map(|path| PathBuf::from(super::render::uri_to_path(path)))
            .find(|path| path.is_absolute() && path.is_file());
    })
    .ok()?;
    found
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

/// A start [`schedule`] makes: an authorized file, the server configuration
/// `lspSearch` would resolve for it, and that server's pool key.
struct Planned {
    file: String,
    key: String,
    config: LspLanguageServerConfig,
}

/// The start for `file` when it is authorized and has a configured server.
fn plan(paths: &PathPolicy, execution: &LspExecutionConfig, file: &Path) -> Option<Planned> {
    let file = paths.validate_read(file).ok()?;
    let file = file.canonical.to_string_lossy().into_owned();
    let workspace = resolve_workspace_root_for_file(file.clone())
        .ok()
        .and_then(|root| paths.validate(&root).ok())
        .filter(|root| root.canonical.is_dir())
        .map(|root| root.canonical.to_string_lossy().into_owned())?;
    let config_path = match execution.config_path.as_deref() {
        Some(path) => Some(paths.validate_read(path).ok()?.canonical),
        None => None,
    };
    let discovery = execution.discovery(config_path);
    let config = default_server_for_file(&file, &workspace, &discovery)?;
    let key = canonical_lsp_key(&config).ok()?;
    Some(Planned { file, key, config })
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
    plan(paths, execution, file).is_some_and(|planned| start(handle, pool, execution, planned))
}

/// Start a planned server unless its key is warm or in flight, or the
/// runtime already runs [`MAX_PREWARM_KEYS`] starts.
fn start(
    handle: &tokio::runtime::Handle,
    pool: &Arc<LspClientPool>,
    execution: &LspExecutionConfig,
    Planned { file, key, config }: Planned,
) -> bool {
    {
        let mut state = execution
            .prewarm_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.begin(&key) {
            return false;
        }
    }
    let pool = pool.clone();
    let state = execution.prewarm_state.clone();
    handle.spawn(async move {
        let success = warm(&pool, config, file).await;
        state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .finish(&key, success);
    });
    true
}

/// Start (or reuse) `config`'s pooled server and open `file` so its project
/// load starts too. `true` only when the server is ready and the file was
/// opened without a readiness timeout.
pub(super) async fn warm(
    pool: &LspClientPool,
    config: LspLanguageServerConfig,
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
            octocode_engine::text::read_source(&path, MAX_PREWARM_OPEN_BYTES)
                .ok()
                .map(|source| source.bytes)
        })
        .await
        .ok()??;
        let content = super::source::admitted_text(&bytes).ok()?;
        let readiness = client
            .open_document_and_wait(file, &content, None, Some(PREWARM_READY_TIMEOUT_MS))
            .await
            .ok()?;
        (readiness.as_deref() != Some(TIMEOUT)).then_some(())
    };
    warmed.await.is_some()
}

pub mod workspace;

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
        let dir_dir = crate::tools::test_support::Fixture::new();
        let dir = dir_dir.0.clone();
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
        let dir_dir = crate::tools::test_support::Fixture::new();
        let dir = dir_dir.0.clone();
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("a.ts");
        std::fs::write(&file, "export const a = 1;\n").expect("file");
        let uri = format!("file://{}", file.display());
        let data = json!({"related":[{"tool":"lspSearch","kind":"related","input":{"queries":[{"path":uri,"symbolName":"a","line":1}]}}]});
        assert_eq!(
            lead_file(crate::id::ToolId::LocalSearch, &data),
            Some(file.clone())
        );
        // A file uri is percent-decoded, as lspSearch itself decodes it.
        let spaced = dir.join("a b.ts");
        std::fs::write(&spaced, "export const b = 1;\n").expect("spaced file");
        let encoded = format!("file://{}/a%20b.ts", dir.display());
        let data = json!({"related":[{"tool":"lspSearch","kind":"related","input":{"queries":[{"path":encoded,"symbolName":"b","line":1}]}}]});
        assert_eq!(
            lead_file(crate::id::ToolId::LocalSearch, &data),
            Some(spaced)
        );
        let other = json!({"related":[{"tool":"localFetch","kind":"related","input":{"queries":[{"path":file}]}}]});
        assert_eq!(lead_file(crate::id::ToolId::LocalSearch, &other), None);
        let relative = json!({"related":[{"tool":"lspSearch","kind":"related","input":{"queries":[{"path":"a.ts"}]}}]});
        assert_eq!(lead_file(crate::id::ToolId::LocalSearch, &relative), None);
        let provider = json!({"files":[{"path":"a.ts","provider":data["related"][0]}]});
        assert_eq!(lead_file(crate::id::ToolId::LocalSearch, &provider), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
