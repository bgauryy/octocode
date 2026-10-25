//! An OS lock serializes publication, latest and retention across CLI processes.
use super::*;

pub(super) fn acquire(
    home: &Path,
    cancel: &dyn CancellationCheck,
    paths: &PathPolicy,
) -> Result<std::fs::File, GraphOutput> {
    let io_error =
        |e: std::io::Error| GraphOutput::error(ErrorCode::FileAccessFailed, e.to_string());
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
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(target)
        .map_err(io_error)?;
    loop {
        cancel
            .check()
            .map_err(|e| GraphOutput::error(ErrorCode::Cancelled, e))?;
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(io_error(e)),
        }
    }
}
