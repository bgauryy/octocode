use super::super::super::graph::checkpoint;
use super::*;

pub(super) struct Staged {
    path: PathBuf,
}

const STAGE_MARKER: &str = ".octocode-graph-stage";
const STAGE_IDENTITY: &[u8] = b"octocode.graph.stage.v1\n";

/// A graph home that cannot be written: `fileAccessFailed` with the recovery
/// of a write, not a read's.
pub(super) fn write_failed(message: impl Into<String>) -> GraphOutput {
    let mut out = GraphOutput::error(ErrorCode::FileAccessFailed, message);
    octocode_runtime_base::response::result::set_hints(
        &mut out.value,
        [octocode_runtime_base::response::rows::WRITE_ACCESS_HINT],
    );
    out
}

pub(super) fn cleanup(
    home: &Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(), GraphOutput> {
    let failed =
        |error: std::io::Error| write_failed(format!("cannot recover graph staging: {error}"));
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
        if octocode_tools_shared::source::read_bounded(&marker, STAGE_IDENTITY.len())
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

use octocode_io::atomic::sync_dir;

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

/// Settles this build's extraction job as the scope's fact cache once the
/// snapshot `receipt` names is published, leaving any newer or still-held job
/// for the same scope alone.
pub(super) fn finish_checkpoint(
    options: &IngestOptions,
    home: &Path,
    built: &super::super::super::types::BuiltGraph,
    receipt: &Value,
) -> Result<(), GraphOutput> {
    if options.memory_only {
        return Ok(());
    }
    let generation = built
        .construction
        .checkpoint_generation
        .as_deref()
        .ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::ExecutionFailed,
                "published build has no extraction job identity",
            )
        })?;
    let job = checkpoint::job_path(&home.join(".facts"), &built.root);
    if let Err(error) = checkpoint::settle(&job, generation) {
        return Err(GraphOutput::error(
            error.code,
            format!(
                "graph {} was published at {}, but its extraction checkpoint could not be settled: {}",
                receipt["graph"].as_str().unwrap_or_default(),
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
pub(in crate::ast_graph::store) fn publish(
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
mod tests;
