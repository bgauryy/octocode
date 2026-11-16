//! An OS lock serializes publication, latest and retention across CLI
//! processes. Builds run outside it: [`super::ingest`] takes it only to
//! stage, publish and prune.
use super::super::graph::checkpoint::lock;
use super::*;

pub(super) fn acquire(
    home: &Path,
    cancel: &dyn CancellationCheck,
    paths: &PathPolicy,
) -> Result<std::fs::File, GraphOutput> {
    // The lock file lives in the home: failing to take it is a write failing.
    let io_error = |e: std::io::Error| {
        let mut out = GraphOutput::error(ErrorCode::FileAccessFailed, e.to_string());
        crate::response::result::set_hints(
            &mut out.value,
            [crate::response::rows::WRITE_ACCESS_HINT],
        );
        out
    };
    let target = home.join(".build.lock");
    let checked = paths
        .validate_output(&target)
        .map_err(|error| GraphOutput::error(error.shared_code(), error.message))?;
    if checked.canonical != target
        || std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            "graph build lock must not be a symlink",
        ));
    }
    std::fs::create_dir_all(home).map_err(io_error)?;
    let file = lock::open(&target).map_err(io_error)?;
    match lock::exclusive(&file, cancel) {
        Ok(()) => Ok(file),
        Err(lock::Wait::Cancelled(message)) => {
            Err(GraphOutput::error(ErrorCode::Cancelled, message))
        }
        Err(lock::Wait::Failed(error)) => Err(io_error(error)),
    }
}
