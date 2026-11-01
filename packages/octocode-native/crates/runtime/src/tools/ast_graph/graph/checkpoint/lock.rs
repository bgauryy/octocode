//! Cancellable exclusive OS file locks: the workspace build lock and each
//! extraction job's lock. A holder that exits releases its lock.
use super::*;

/// Why a lock wait ended without the lock.
pub(in crate::tools::ast_graph) enum Wait {
    Cancelled(String),
    Failed(std::io::Error),
}

/// Opens (creating) the lock file at `path`; its contents are never read.
pub(in crate::tools::ast_graph) fn open(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
}

/// Waits for the exclusive lock on `file` until `cancel` stops the wait.
pub(in crate::tools::ast_graph) fn exclusive(
    file: &std::fs::File,
    cancel: &dyn CancellationCheck,
) -> Result<(), Wait> {
    loop {
        cancel.check().map_err(Wait::Cancelled)?;
        match file.try_lock() {
            Ok(()) => return Ok(()),
            // A poll interval, not a bound: the wait lasts until the holder
            // releases or `cancel` stops it.
            Err(std::fs::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(Wait::Failed(error)),
        }
    }
}

/// An extraction job's lock file, beside (never inside) the job directory,
/// so resetting the directory never unlinks a held lock.
pub(in crate::tools::ast_graph) fn job_lock_path(job: &Path) -> PathBuf {
    job.with_extension("lock")
}

impl Wait {
    pub(super) fn into_tool_error(self) -> ToolError {
        match self {
            Self::Cancelled(message) => ToolError::new(ErrorCode::Cancelled, message),
            Self::Failed(error) => failure(format!("cannot lock graph checkpoint: {error}")),
        }
    }
}
