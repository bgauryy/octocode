//! The files changed since a git revision.

use super::*;

/// Root-relative files changed since `rev` (committed, staged, unstaged) plus
/// untracked files, via git. Only the request's cancellation bounds the runs:
/// a large repository or a cold filesystem is valid work, and the output is
/// read whole.
pub(super) fn git_changed(
    root: &Path,
    rev: &str,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Vec<String>, GraphOutput> {
    if rev.starts_with('-') {
        return Err(GraphOutput::error(
            ErrorCode::GitFailed,
            format!("invalid revision {rev:?}"),
        ));
    }
    let run = |args: &[&str]| -> Result<Vec<String>, GraphOutput> {
        let mut command = std::process::Command::new("git");
        // The repository's own config must not run programs: an fsmonitor
        // hook executes on every index read. No prompts, no index rewrites.
        command
            .arg("-C")
            .arg(root)
            .args(["-c", "core.fsmonitor=false"])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0");
        let output = crate::tools::bounded_process::run_bounded(
            command,
            "git",
            std::time::Duration::MAX,
            usize::MAX,
            cancel,
        )
        .map_err(|error| {
            use crate::tools::bounded_process::BoundedError;
            match error {
                BoundedError::Cancelled(message) => {
                    GraphOutput::error(ErrorCode::Cancelled, message)
                }
                BoundedError::TimedOut(message) | BoundedError::Failed(message) => {
                    GraphOutput::error(ErrorCode::GitFailed, message)
                }
            }
        })?;
        if !output.status.success() {
            return Err(GraphOutput::error(
                ErrorCode::GitFailed,
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    };
    let mut files = run(&["diff", "--name-only", "--relative", rev, "--"])?;
    files.extend(run(&["ls-files", "--others", "--exclude-standard"])?);
    files.sort();
    files.dedup();
    Ok(files)
}
