use super::super::super::graph::checkpoint;
use super::*;

pub(super) struct Staged {
    path: PathBuf,
}

const STAGE_MARKER: &str = ".octocode-graph-stage";
const STAGE_IDENTITY: &[u8] = b"octocode.graph.stage.v1\n";

pub(super) fn cleanup(
    home: &Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(), GraphOutput> {
    let failed = |error: std::io::Error| {
        GraphOutput::error(
            ErrorCode::FileAccessFailed,
            format!("cannot recover graph staging: {error}"),
        )
    };
    for entry in std::fs::read_dir(home).map_err(failed)? {
        cancel
            .check()
            .map_err(|error| GraphOutput::error(ErrorCode::Cancelled, error))?;
        let entry = entry.map_err(failed)?;
        let name = entry.file_name();
        let Some(nonce) = name
            .to_str()
            .and_then(|name| name.strip_prefix(".tmp-"))
            .and_then(|name| name.strip_suffix(".graph"))
        else {
            continue;
        };
        if nonce.len() != 32
            || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !entry.file_type().map_err(failed)?.is_dir()
        {
            continue;
        }
        let path = entry.path();
        let valid = paths
            .validate_output(&path)
            .map_err(|error| GraphOutput::error(error.shared_code(), error.message))?;
        if valid.canonical != path {
            continue;
        }
        let marker = path.join(STAGE_MARKER);
        if !std::fs::symlink_metadata(&marker).is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        if crate::tools::source::read_bounded(&marker, STAGE_IDENTITY.len())
            .ok()
            .as_deref()
            != Some(STAGE_IDENTITY)
        {
            continue;
        }
        std::fs::remove_dir_all(path).map_err(failed)?;
        sync_dir(home).map_err(failed)?;
    }
    Ok(())
}

fn sync_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

impl Staged {
    pub(super) fn new(home: &Path) -> std::io::Result<Self> {
        octocode_io::atomic::create_private_dir_all(home)?;
        let path = octocode_io::atomic::unique_sibling(&home.join("tmp"), "graph")?;
        octocode_io::atomic::create_private_dir_all(&path)?;
        let staged = Self { path };
        write_atomic(&staged.path.join(STAGE_MARKER), STAGE_IDENTITY, true)?;
        sync_dir(home)?;
        Ok(staged)
    }

    pub(super) fn graph(&self) -> PathBuf {
        self.path.join(GRAPH_FILE)
    }

    pub(super) fn publish(
        &self,
        home: &Path,
        id: &str,
        manifest: &Value,
    ) -> std::io::Result<PathBuf> {
        std::fs::File::open(self.graph())?.sync_all()?;
        let bytes = serde_json::to_vec_pretty(manifest).map_err(std::io::Error::other)?;
        write_atomic(&self.path.join(MANIFEST_FILE), &bytes, true)?;
        sync_dir(&self.path)?;
        let target = home.join(id);
        std::fs::rename(&self.path, &target)?;
        let committed = |error: std::io::Error| {
            std::io::Error::new(
                error.kind(),
                format!(
                    "snapshot {} is committed, but latest publication did not complete: {error}",
                    target.display()
                ),
            )
        };
        sync_dir(home).map_err(committed)?;
        std::fs::remove_file(target.join(STAGE_MARKER)).map_err(committed)?;
        sync_dir(&target).map_err(committed)?;
        write_atomic(&home.join(LATEST_FILE), id.as_bytes(), true).map_err(committed)?;
        sync_dir(home).map_err(committed)?;
        Ok(target)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The build lock with abandoned staging removed. Staging exists only under
/// this lock, so every stage [`cleanup`] finds is one a dead process left.
pub(super) fn lock(
    home: &Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<std::fs::File, GraphOutput> {
    let lock = super::super::build_lock::acquire(home, cancel, paths)?;
    cleanup(home, paths, cancel)?;
    Ok(lock)
}

/// Points `latest` at the reused snapshot `receipt` names; under the lock.
pub(super) fn point_latest(home: &Path, mut receipt: Value, started: Instant) -> GraphOutput {
    let id = receipt["id"].as_str().unwrap_or_default();
    if let Err(error) = write_atomic(&home.join(LATEST_FILE), id.as_bytes(), false) {
        return GraphOutput::error(
            ErrorCode::FileAccessFailed,
            format!("cannot point latest at graph {id}: {error}"),
        );
    }
    crate::tools::ast_graph::progress::emit(|| format!("reused {id}: its sources are unchanged"));
    receipt["totalMs"] = json!(crate::num::millis_of(started.elapsed()));
    GraphOutput::ok(receipt)
}

/// Marks the scope's extraction job complete once the snapshot `receipt`
/// names is published.
pub(super) fn finish_checkpoint(
    options: &IngestOptions,
    home: &Path,
    built: &super::super::super::types::BuiltGraph,
    receipt: &Value,
) -> Result<(), GraphOutput> {
    let job = checkpoint::job_path(&home.join(".facts"), &built.root);
    if !options.memory_only
        && let Err(error) = checkpoint::complete(&job)
    {
        return Err(GraphOutput::error(
            error.code,
            format!(
                "graph {} was published at {}, but its extraction checkpoint could not be marked complete: {}",
                receipt["id"].as_str().unwrap_or_default(),
                receipt["dir"].as_str().unwrap_or_default(),
                error.message
            ),
        ));
    }
    Ok(())
}

/// Writes the snapshot under a temp dir, renames it into place and points
/// the latest marker at it.
#[cfg(test)]
pub(in crate::tools::ast_graph::store) fn publish(
    home: &Path,
    id: &str,
    _stamp: &str,
    bytes: &[u8],
    manifest: &Value,
) -> std::io::Result<PathBuf> {
    let staged = Staged::new(home)?;
    write_atomic(&staged.graph(), bytes, true)?;
    staged.publish(home, id, manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_staging_is_removed_and_previous_latest_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LATEST_FILE), b"previous").unwrap();
        let staged = Staged::new(dir.path()).unwrap();
        let path = staged.path.clone();
        std::fs::write(staged.graph(), b"partial").unwrap();
        drop(staged);
        assert!(!path.exists());
        assert_eq!(
            std::fs::read(dir.path().join(LATEST_FILE)).unwrap(),
            b"previous"
        );
    }
    #[test]
    fn failed_publication_preserves_previous_latest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LATEST_FILE), b"previous").unwrap();
        let staged = Staged::new(dir.path()).unwrap();
        assert!(staged.publish(dir.path(), "next", &json!({})).is_err());
        assert_eq!(
            std::fs::read(dir.path().join(LATEST_FILE)).unwrap(),
            b"previous"
        );
    }

    #[test]
    fn abandoned_stage_cleanup_preserves_unmarked_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let staged = std::mem::ManuallyDrop::new(Staged::new(&root).unwrap());
        std::fs::write(staged.graph(), b"partial").unwrap();
        let unrelated = root.join(".tmp-0123456789abcdef0123456789abcdef.graph");
        std::fs::create_dir(&unrelated).unwrap();
        cleanup(
            &root,
            &crate::tools::test_support::workspace_policy(&root),
            &crate::execution::cancel::NeverCancel,
        )
        .unwrap();
        assert!(!staged.path.exists());
        assert!(unrelated.is_dir());
    }

    #[test]
    fn failed_latest_update_reports_the_already_committed_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(LATEST_FILE)).unwrap();
        let staged = Staged::new(dir.path()).unwrap();
        std::fs::write(staged.graph(), b"complete graph").unwrap();
        let error = staged
            .publish(dir.path(), "committed", &json!({"complete":true}))
            .unwrap_err();
        assert!(error.to_string().contains("is committed"));
        assert_eq!(
            std::fs::read(dir.path().join("committed").join(GRAPH_FILE)).unwrap(),
            b"complete graph"
        );
        assert!(dir.path().join("committed").join(MANIFEST_FILE).is_file());
    }
}
