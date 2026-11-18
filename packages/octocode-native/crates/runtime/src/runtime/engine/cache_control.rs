//! Cache maintenance is explicit and runs after this runtime drains.
use super::{RuntimeError, ToolRuntime};
use octocode_contract_types::ErrorCode;
use octocode_tool_graph::ast_graph::store;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearResult {
    pub removed: Vec<PathBuf>,
    pub restart_mcp: bool,
}

impl ToolRuntime {
    /// Drain and close this runtime, then remove its caches. Independent MCP
    /// processes must be stopped before clearing and restarted afterwards.
    /// Clones, published graphs, credentials and installed servers are outputs.
    pub async fn clear_caches(&self) -> Result<CacheClearResult, RuntimeError> {
        self.close().await;
        octocode_engine::memory::MEMORY.clear();
        let home = self.config.home.clone();
        let workspace = self.graph_workspace(None);
        let graph = store::graph_home(
            Some(&workspace),
            self.config
                .resolved
                .local
                .graph_home
                .as_deref()
                .map(Path::new),
        )
        .map_err(|message| RuntimeError::new(ErrorCode::WorkspaceRootInvalid, message))?;
        let removed = tokio::task::spawn_blocking(move || clear_disk(&home, &workspace, &graph))
            .await
            .map_err(|error| RuntimeError::new(ErrorCode::ExecutionFailed, error.to_string()))?
            .map_err(|error| RuntimeError::new(ErrorCode::FileAccessFailed, error.to_string()))?;
        Ok(CacheClearResult {
            removed,
            restart_mcp: true,
        })
    }
}

/// `graph` is the snapshot home ([`store::graph_home`]); only its
/// extraction checkpoints go, never a published snapshot.
fn clear_disk(home: &Path, workspace: &Path, graph: &Path) -> std::io::Result<Vec<PathBuf>> {
    let directories: Vec<_> = octocode_runtime_base::cache::Directory::ALL
        .into_iter()
        .map(|dir| dir.path(home))
        // Retired result stores have no readers or writers.
        .chain(octocode_runtime_base::cache::Directory::retired(home))
        .chain([
            octocode_tool_github::gh_structure::materialize::materialize_root(home),
            octocode_tools_shared::source_capture::url::capture_root(home),
            graph.join(".facts"),
        ])
        .collect();
    for path in &directories {
        for parent in path
            .ancestors()
            .take_while(|parent| *parent != home && *parent != workspace && *parent != graph)
        {
            if std::fs::symlink_metadata(parent).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(std::io::Error::other(format!(
                    "refusing to clear through symlink {}",
                    parent.display()
                )));
            }
        }
    }
    let mut removed = Vec::new();
    for path in directories {
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if metadata.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
        removed.push(path);
    }
    Ok(removed)
}

#[cfg(test)]
mod tests;
