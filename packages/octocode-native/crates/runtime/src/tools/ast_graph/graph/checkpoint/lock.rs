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

/// A poll interval, not a bound: a wait lasts until the holder releases or
/// its cancellation stops it.
const POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// Waits for the exclusive lock on `file` until `cancel` stops the wait.
pub(in crate::tools::ast_graph) fn exclusive(
    file: &std::fs::File,
    cancel: &dyn CancellationCheck,
) -> Result<(), Wait> {
    loop {
        cancel.check().map_err(Wait::Cancelled)?;
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(POLL),
            Err(std::fs::TryLockError::Error(error)) => return Err(Wait::Failed(error)),
        }
    }
}

/// Waits for the lock of the extraction job whose lock file is `path` until
/// `cancel` stops the wait. A finished or abandoned job's lock file is
/// removed by its holder ([`octocode_io::lock::LockedFile::remove_held`]),
/// so a lock taken on a file the path no longer names is dropped and taken
/// again: every holder of a job holds the one file its path names.
pub(in crate::tools::ast_graph) fn job(
    path: &Path,
    cancel: &dyn CancellationCheck,
) -> Result<octocode_io::lock::LockedFile, Wait> {
    loop {
        let mut file = octocode_io::lock::open_lock_file(path).map_err(Wait::Failed)?;
        let held = loop {
            cancel.check().map_err(Wait::Cancelled)?;
            match octocode_io::lock::try_lock(file).map_err(Wait::Failed)? {
                octocode_io::lock::TryLock::Locked(held) => break held,
                octocode_io::lock::TryLock::Busy(busy) => {
                    file = busy;
                    #[cfg_attr(
                        octocode_clippy_ratchet,
                        expect(
                            clippy::disallowed_methods,
                            reason = "The synchronous graph worker polls an OS file lock and checks cancellation each time; this interval never caps the wait"
                        )
                    )]
                    std::thread::sleep(POLL);
                }
            }
        };
        if held.still_at(path) {
            return Ok(held);
        }
    }
}

/// The job lock at `path` if no build holds it now, taken on the file the
/// path names; `Ok(None)` while another build holds it.
pub(in crate::tools::ast_graph) fn try_job(
    path: &Path,
) -> std::io::Result<Option<octocode_io::lock::LockedFile>> {
    let file = octocode_io::lock::open_lock_file(path)?;
    Ok(match octocode_io::lock::try_lock(file)? {
        octocode_io::lock::TryLock::Locked(held) => held.still_at(path).then_some(held),
        octocode_io::lock::TryLock::Busy(_) => None,
    })
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
