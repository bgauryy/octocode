//! The files changed since a git revision.

use super::*;

/// How a path changed since a revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Change {
    Added,
    Deleted,
    Modified,
}

impl Change {
    /// Git's `--name-status` letter as a change: a rename is a deletion and
    /// an addition (`--no-renames`); a type change or an unmerged path is a
    /// modification.
    fn of_status(status: &str) -> Self {
        match status.as_bytes().first() {
            Some(b'A') => Self::Added,
            Some(b'D') => Self::Deleted,
            _ => Self::Modified,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Deleted => "deleted",
            Self::Modified => "modified",
        }
    }
}

/// Root-relative files changed since `rev` (committed, staged, unstaged) plus
/// untracked files (added), each with how it changed, in path order, via git.
/// Only the request's cancellation bounds the runs: a large repository or a
/// cold filesystem is valid work, and the output is read whole.
pub(super) fn git_changed(
    root: &Path,
    rev: &str,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<Vec<(String, Change)>, GraphOutput> {
    // A revision git cannot name is the caller's to correct: an option
    // spelling, or no commit of this repository. Any other git failure
    // (no repository, no git) is the tree's.
    let unknown = || {
        GraphOutput::error(
            ErrorCode::InvalidInput,
            format!(
                "since {rev:?} names no commit of the repository at the graph's root; pass a branch, tag or commit"
            ),
        )
    };
    if rev.starts_with('-') {
        return Err(unknown());
    }
    let commit = format!("{rev}^{{commit}}");
    let verified = git_output(root, &["rev-parse", "--verify", "--quiet", &commit], cancel)?;
    // `--verify --quiet` exits 1, silently, for a name that is no commit.
    if verified.status.code() == Some(1) {
        return Err(unknown());
    }
    success(verified)?;
    let run = |args: &[&str]| -> Result<Vec<String>, GraphOutput> {
        // NUL-separated (`-z`): paths arrive unquoted, whatever they hold.
        Ok(String::from_utf8_lossy(&git(root, args, cancel)?)
            .split('\0')
            .filter(|field| !field.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>())
    };
    let diff = run(&[
        "diff",
        "--name-status",
        "-z",
        "--no-renames",
        "--relative",
        rev,
        "--",
    ])?;
    let mut files = diff
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[status, path]| (path.clone(), Change::of_status(status)))
        .collect::<Vec<_>>();
    files.extend(
        run(&["ls-files", "-z", "--others", "--exclude-standard"])?
            .into_iter()
            .map(|path| (path, Change::Added)),
    );
    files.sort();
    files.dedup_by(|a, b| a.0 == b.0);
    Ok(files)
}

/// The standard output of `git -C root <args>`, read whole. The repository's
/// own config must not run programs: an fsmonitor hook executes on every
/// index read. No prompts, no index rewrites. Only the request's
/// cancellation bounds the run.
pub(in super::super) fn git(
    root: &Path,
    args: &[&str],
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<Vec<u8>, GraphOutput> {
    success(git_output(root, args, cancel)?)
}

/// The standard output of a git run that succeeded; its standard error
/// otherwise.
fn success(
    output: octocode_tools_shared::bounded_process::BoundedOutput,
) -> Result<Vec<u8>, GraphOutput> {
    if !output.status.success() {
        return Err(GraphOutput::error(
            ErrorCode::GitFailed,
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(output.stdout)
}

/// [`git`]'s run, whatever its exit status.
fn git_output(
    root: &Path,
    args: &[&str],
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
) -> Result<octocode_tools_shared::bounded_process::BoundedOutput, GraphOutput> {
    let mut command = std::process::Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(["-c", "core.fsmonitor=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    octocode_tools_shared::bounded_process::run_bounded(
        command,
        "git",
        std::time::Duration::MAX,
        usize::MAX,
        cancel,
    )
    .map_err(|error| {
        use octocode_tools_shared::bounded_process::BoundedError;
        match error {
            BoundedError::Cancelled(message) => GraphOutput::error(ErrorCode::Cancelled, message),
            BoundedError::TimedOut(message) | BoundedError::Failed(message) => {
                GraphOutput::error(ErrorCode::GitFailed, message)
            }
        }
    })
}
