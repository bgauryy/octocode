use super::{CloneContext, CloneError, GitRunRequest, control, is_commit};
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

const CLONE_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const SPARSE_TIMEOUT: Duration = Duration::from_secs(30);
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn read_head(
    context: &CloneContext<'_>,
    directory: &Path,
) -> Result<String, CloneError> {
    let output = run(
        context,
        vec![
            "-C".into(),
            directory.as_os_str().to_owned(),
            "rev-parse".into(),
            "--verify".into(),
            "HEAD^{commit}".into(),
        ],
        HEAD_TIMEOUT,
        "read checkout HEAD",
        None,
    )?;
    let sha = output.stdout.trim().to_ascii_lowercase();
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CloneError::new(
            "clone.git.invalidHead",
            "Git returned an invalid checkout HEAD commit SHA.",
        ));
    }
    Ok(sha)
}

/// Whether a cached checkout's working tree still matches its HEAD commit.
/// Edited, deleted, or added files mean the cache no longer holds the fetched
/// revision, so it must not be served as `verified`.
pub(super) fn is_clean(context: &CloneContext<'_>, directory: &Path) -> Result<bool, CloneError> {
    let output = run(
        context,
        vec![
            "-C".into(),
            directory.as_os_str().to_owned(),
            "status".into(),
            "--porcelain".into(),
            "--untracked-files=normal".into(),
        ],
        HEAD_TIMEOUT,
        "check cached checkout cleanliness",
        None,
    )?;
    // Porcelain lines are `XY <path>`; octocode's own cache bookkeeping
    // (meta, lock, and their temp files) lives in the checkout root.
    Ok(output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .all(|line| {
            line.get(3..)
                .is_some_and(|path| path.trim_matches('"').starts_with(".octocode-"))
        }))
}

pub(super) fn checkout(
    context: &CloneContext<'_>,
    repository_url: &str,
    branch: &str,
    sparse_path: Option<&str>,
    target: &Path,
) -> Result<(), CloneError> {
    if is_commit(branch) {
        checkout_commit(context, repository_url, branch, sparse_path, target)
    } else if let Some(sparse_path) = sparse_path {
        checkout_sparse(context, repository_url, branch, sparse_path, target)
    } else {
        checkout_full(context, repository_url, branch, target)
    }
}

fn checkout_full(
    context: &CloneContext<'_>,
    repository_url: &str,
    branch: &str,
    target: &Path,
) -> Result<(), CloneError> {
    run(
        context,
        vec![
            "clone".into(),
            "--depth".into(),
            "1".into(),
            "--single-branch".into(),
            "--branch".into(),
            branch.into(),
            "--".into(),
            repository_url.into(),
            target.as_os_str().to_owned(),
        ],
        CLONE_TIMEOUT,
        "full clone",
        Some(repository_url),
    )?;
    Ok(())
}

fn checkout_sparse(
    context: &CloneContext<'_>,
    repository_url: &str,
    branch: &str,
    sparse_path: &str,
    target: &Path,
) -> Result<(), CloneError> {
    run(
        context,
        vec![
            "clone".into(),
            "--filter".into(),
            "blob:none".into(),
            "--sparse".into(),
            "--depth".into(),
            "1".into(),
            "--single-branch".into(),
            "--branch".into(),
            branch.into(),
            "--".into(),
            repository_url.into(),
            target.as_os_str().to_owned(),
        ],
        CLONE_TIMEOUT,
        "sparse clone",
        Some(repository_url),
    )?;
    run(
        context,
        vec![
            "-C".into(),
            target.as_os_str().to_owned(),
            "sparse-checkout".into(),
            "set".into(),
            "--skip-checks".into(),
            "--".into(),
            sparse_path.into(),
        ],
        SPARSE_TIMEOUT,
        "set sparse checkout paths",
        Some(repository_url),
    )?;
    Ok(())
}

fn checkout_commit(
    context: &CloneContext<'_>,
    repository_url: &str,
    commit: &str,
    sparse_path: Option<&str>,
    target: &Path,
) -> Result<(), CloneError> {
    run(
        context,
        vec!["init".into(), "--".into(), target.as_os_str().to_owned()],
        CLONE_TIMEOUT,
        "initialize commit checkout",
        None,
    )?;
    run(
        context,
        scoped(target, &["remote", "add", "origin", repository_url]),
        CLONE_TIMEOUT,
        "configure commit remote",
        Some(repository_url),
    )?;
    if sparse_path.is_some() {
        run(
            context,
            scoped(target, &["config", "remote.origin.promisor", "true"]),
            SPARSE_TIMEOUT,
            "configure sparse promisor",
            Some(repository_url),
        )?;
        run(
            context,
            scoped(
                target,
                &["config", "remote.origin.partialclonefilter", "blob:none"],
            ),
            SPARSE_TIMEOUT,
            "configure sparse filter",
            Some(repository_url),
        )?;
    }
    let mut fetch = scoped(target, &["fetch", "--depth", "1"]);
    if sparse_path.is_some() {
        fetch.extend([OsString::from("--filter"), OsString::from("blob:none")]);
    }
    fetch.extend([
        OsString::from("--"),
        OsString::from("origin"),
        OsString::from(commit),
    ]);
    run(
        context,
        fetch,
        CLONE_TIMEOUT,
        "fetch requested commit",
        Some(repository_url),
    )?;
    if let Some(sparse_path) = sparse_path {
        let mut args = scoped(
            target,
            &["sparse-checkout", "set", "--cone", "--skip-checks", "--"],
        );
        args.push(sparse_path.into());
        run(
            context,
            args,
            SPARSE_TIMEOUT,
            "set sparse commit paths",
            Some(repository_url),
        )?;
    }
    run(
        context,
        scoped(target, &["checkout", "--detach", "FETCH_HEAD", "--"]),
        CLONE_TIMEOUT,
        "check out requested commit",
        Some(repository_url),
    )?;
    Ok(())
}

fn scoped(target: &Path, args: &[&str]) -> Vec<OsString> {
    let mut result = vec!["-C".into(), target.as_os_str().to_owned()];
    result.extend(args.iter().map(OsString::from));
    result
}

/// Global options prepended to every git invocation. `core.symlinks=false`
/// makes clone/fetch/checkout materialize repository symlinks as plain files
/// holding the link text, so a hostile repo cannot point into other
/// `~/.octocode` state that localFetch would then read through the clone.
const HARDENING_ARGS: [&str; 2] = ["-c", "core.symlinks=false"];

/// Longest cooldown a clone waits out before failing fast.
const GIT_COOLDOWN_CAP: Duration = Duration::from_secs(10);

fn git_permit(
    context: &CloneContext<'_>,
    repository_url: &str,
    token: Option<&str>,
) -> Result<tokio::sync::OwnedSemaphorePermit, CloneError> {
    use crate::providers::github::{GitHubBudget, LimiterKey, ProviderErrorKind};
    let Ok(url) = url::Url::parse(repository_url) else {
        return Err(CloneError::new(
            "clone.input.invalid",
            "Repository URL could not be parsed",
        ));
    };
    let budget = GitHubBudget::global();
    let state_dir = context
        .config
        .persistent
        .then(|| context.config.cache_home.join("tmp").join("ratelimit"));
    let state = budget.key_state(&LimiterKey::for_url(&url, token), state_dir.as_deref());
    state
        .acquire_git_blocking(budget.config(), GIT_COOLDOWN_CAP, context.deadline, &|| {
            context.cancellation.check().is_err()
        })
        .map_err(|error| match error.kind {
            ProviderErrorKind::Cancelled => {
                CloneError::new("clone.execution.cancelled", error.message.to_string())
            }
            ProviderErrorKind::Timeout => {
                CloneError::new("clone.execution.timeout", error.message.to_string())
            }
            _ => CloneError::new(
                "clone.rateLimited",
                format!(
                    "{} Retry after {}s.",
                    error.message,
                    error
                        .rate_limit
                        .and_then(|rate| rate.retry_after_seconds)
                        .unwrap_or(60)
                ),
            ),
        })
}

fn run(
    context: &CloneContext<'_>,
    args: Vec<OsString>,
    timeout: Duration,
    label: &str,
    authorization_url: Option<&str>,
) -> Result<super::GitOutput, CloneError> {
    let args = HARDENING_ARGS
        .iter()
        .map(OsString::from)
        .chain(args)
        .collect();
    let authorization = context.credential.map(|credential| credential.expose());
    // Network git operations share the host/token executor: honor its
    // secondary-limit cooldown and cap concurrent clones via the `git` group.
    let _git_permit = match authorization_url {
        Some(url) => Some(git_permit(context, url, authorization)?),
        None => None,
    };
    context.git.run(
        &GitRunRequest {
            args,
            timeout,
            label: label.into(),
            authorization,
            authorization_url,
        },
        &control(context),
    )
}
