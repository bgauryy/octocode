//! Cache maintenance is explicit and runs after this runtime drains.
use super::{RuntimeError, ToolRuntime};
use crate::tools::ast_graph::store;
use octocode_contract_types::ErrorCode;
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
    let directories: Vec<_> = crate::cache::Directory::ALL
        .into_iter()
        .map(|dir| dir.path(home))
        // Retired result stores have no readers or writers. Explicit clearing
        // also removes artifacts left by an older installed binary.
        .chain(
            ["response", "outline", "artifact", "judgment"].map(|name| home.join("tmp").join(name)),
        )
        .chain([
            home.join("cache/provider-answers-v1"),
            crate::tools::gh_structure::materialize::materialize_root(home),
            crate::tools::source_capture::url::capture_root(home),
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
mod tests {
    use super::*;

    #[test]
    fn clearing_removes_every_store_and_checkpoint_but_keeps_user_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let workspace = dir.path().join("work");
        let cache_dirs: Vec<_> = crate::cache::Directory::ALL
            .into_iter()
            .map(|kind| kind.path(&home))
            .chain(
                ["response", "outline", "artifact", "judgment"]
                    .map(|name| home.join("tmp").join(name)),
            )
            .chain([
                home.join("cache/provider-answers-v1"),
                crate::tools::gh_structure::materialize::materialize_root(&home),
                crate::tools::source_capture::url::capture_root(&home),
                workspace.join(".octocode/graph/.facts"),
            ])
            .collect();
        for path in &cache_dirs {
            std::fs::create_dir_all(path).unwrap();
            std::fs::write(path.join("evidence"), "cache").unwrap();
        }
        let kept = [
            home.join("clones/o/r/local-edit"),
            home.join("tmp/clone/o/r/local-edit"),
            home.join("credentials.json"),
            home.join("tmp/ratelimit/budget"),
            workspace.join(".octocode/graph/published/graph.bin"),
        ];
        for path in &kept {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "user").unwrap();
        }
        let removed = clear_disk(&home, &workspace, &workspace.join(".octocode/graph")).unwrap();
        assert_eq!(removed.len(), cache_dirs.len());
        for path in cache_dirs {
            assert!(!path.exists(), "{}", path.display());
        }
        for path in kept {
            assert_eq!(std::fs::read_to_string(path).unwrap(), "user");
        }
        assert!(
            clear_disk(&home, &workspace, &workspace.join(".octocode/graph"))
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(unix)]
    #[test]
    fn clearing_refuses_a_symlinked_cache_parent_without_touching_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(outside.join("response")).unwrap();
        let file = outside.join("response/user");
        std::fs::write(&file, "keep").unwrap();
        std::os::unix::fs::symlink(&outside, home.join("tmp")).unwrap();
        assert!(clear_disk(&home, dir.path(), &dir.path().join(".octocode/graph")).is_err());
        assert_eq!(std::fs::read_to_string(file).unwrap(), "keep");
    }
}
