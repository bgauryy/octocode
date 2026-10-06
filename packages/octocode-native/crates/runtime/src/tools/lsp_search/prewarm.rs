//! Opt-in language-server prewarm (`OCTOCODE_LSP_PREWARM=1`).
//!
//! A local read or search names a source file before an agent asks for its
//! identity. When enabled, that file's server is started in the shared pool
//! in the background, under exactly the configuration `lspSearch` would
//! resolve for the same file (same workspace root, same discovery options,
//! no `rustContext` overlay, so rust-analyzer keeps its no-build-scripts /
//! no-proc-macros defaults), and the file is opened so a project load starts
//! too. The later `lspSearch` then acquires the warm pooled client.
//!
//! Bounded: at most [`MAX_PREWARM_KEYS`] distinct servers per process, one
//! in-flight start per server, and nothing is ever awaited by the caller.

use super::LspExecutionConfig;
use crate::policy::path::PathPolicy;
use octocode_engine::lsp::config::default_server_for_file;
use octocode_engine::lsp::pool::{LspClientPool, canonical_lsp_key};
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Distinct servers one process may prewarm.
const MAX_PREWARM_KEYS: usize = 3;
/// Bound on the background open's project-load wait.
const PREWARM_READY_TIMEOUT_MS: u32 = 20_000;
const PREWARM_SETTLE_MS: u32 = 400;
/// Largest file the prewarm opens (the anchor read applies the real bound).
const MAX_PREWARM_OPEN_BYTES: u64 = 2 * 1024 * 1024;

/// Whether `OCTOCODE_LSP_PREWARM` enables prewarm (`1`/`true`/`yes`/`on`).
#[must_use]
pub fn enabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

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

/// Whether a response that points at `lspSearch` may warm that call's
/// server: on unless `OCTOCODE_LSP_PREWARM` is explicitly off
/// (`0`/`false`/`no`/`off`).
#[must_use]
pub fn targeted_enabled(value: Option<&str>) -> bool {
    !value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
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

/// Keys started (or starting) by this process.
fn started() -> &'static Mutex<HashSet<String>> {
    static STARTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    STARTED.get_or_init(|| Mutex::new(HashSet::new()))
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
        let Ok(mut keys) = started().lock() else {
            return false;
        };
        if keys.contains(&key) || keys.len() >= MAX_PREWARM_KEYS {
            return false;
        }
        keys.insert(key.clone());
    }
    let pool = pool.clone();
    handle.spawn(async move {
        let warmed = async {
            let client = pool.acquire(config).await.ok().flatten()?;
            let size = tokio::fs::metadata(&file).await.ok()?.len();
            if size > MAX_PREWARM_OPEN_BYTES {
                return Some(());
            }
            let content = tokio::fs::read_to_string(&file).await.ok()?;
            client
                .open_document_and_wait(
                    file,
                    content,
                    Some(PREWARM_SETTLE_MS),
                    Some(PREWARM_READY_TIMEOUT_MS),
                )
                .await
                .ok()
                .map(|_| ())
        }
        .await;
        // A failed start may be retried by a later local call.
        if warmed.is_none()
            && let Ok(mut keys) = started().lock()
        {
            keys.remove(&key);
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::{anchor_file, enabled, lead_file, targeted_enabled};
    use serde_json::json;

    #[test]
    fn prewarm_is_off_unless_explicitly_enabled() {
        assert!(!enabled(None));
        assert!(!enabled(Some("")));
        assert!(!enabled(Some("0")));
        assert!(!enabled(Some("false")));
        for on in ["1", "true", "YES", " on "] {
            assert!(enabled(Some(on)), "{on}");
        }
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
    fn targeted_prewarm_is_on_unless_explicitly_off() {
        assert!(targeted_enabled(None));
        assert!(targeted_enabled(Some("1")));
        for off in ["0", "false", "NO", " off "] {
            assert!(!targeted_enabled(Some(off)), "{off}");
        }
    }

    #[test]
    fn a_lead_naming_lsp_search_points_at_its_anchor_file() {
        let dir =
            std::env::temp_dir().join(format!("octocode-prewarm-lead-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("a.ts");
        std::fs::write(&file, "export const a = 1;\n").expect("file");
        let uri = format!("file://{}", file.display());
        let data = json!({"results":[{"file":"a.ts"}],"next":{"verifyReferences":{"tool":"lspSearch","query":{"queries":[{"path":uri,"symbolName":"a","lineHint":1}]}}}});
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
