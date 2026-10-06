use super::{CloneContext, CloneError, GitRunRequest, cache, control, is_commit};
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

/// A clone or checkout fetch: a few network round trips plus transfer.
fn clone_timeout(context: &CloneContext<'_>) -> Duration {
    context.config.network_timeout.saturating_mul(4)
}

/// One sparse-checkout or fetch step.
fn sparse_timeout(context: &CloneContext<'_>) -> Duration {
    context.config.network_timeout
}

/// A local `rev-parse`/`status` on the checkout.
fn head_timeout(context: &CloneContext<'_>) -> Duration {
    context.config.network_timeout / 6
}

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
        head_timeout(context),
        "read checkout HEAD",
        None,
    )?;
    let sha = output.stdout.trim().to_ascii_lowercase();
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CloneError::new(
            "gitFailed",
            "Git returned an invalid checkout HEAD commit SHA.",
        ));
    }
    Ok(sha)
}

/// Whether a cached checkout's working tree still matches its HEAD commit.
/// Edited, deleted, or added files mean the cache no longer holds the fetched
/// revision, so it must not be served as `verified`.
pub(super) fn is_clean(context: &CloneContext<'_>, directory: &Path) -> Result<bool, CloneError> {
    Ok(checkout_status(context, directory)?.clean)
}

pub(super) struct CheckoutStatus {
    pub clean: bool,
    pub safe_to_replace: bool,
}

/// Inspect ignored files as well: they do not change the verified revision,
/// but replacing the checkout would still destroy them.
pub(super) fn checkout_status(
    context: &CloneContext<'_>,
    directory: &Path,
) -> Result<CheckoutStatus, CloneError> {
    let output = run(
        context,
        vec![
            "-C".into(),
            directory.as_os_str().to_owned(),
            "status".into(),
            "--porcelain=v1".into(),
            "-z".into(),
            "--untracked-files=all".into(),
            "--ignored=matching".into(),
        ],
        head_timeout(context),
        "check cached checkout cleanliness",
        None,
    )?;
    let mut status = CheckoutStatus {
        clean: true,
        safe_to_replace: true,
    };
    for entry in output.stdout.split('\0').filter(|entry| !entry.is_empty()) {
        // Only our untracked root metadata is bookkeeping. A tracked edit,
        // nested namesake or arbitrary .octocode-* file remains user evidence.
        if entry
            .strip_prefix("?? ")
            .or_else(|| entry.strip_prefix("!! "))
            == Some(cache::META_FILE)
        {
            continue;
        }
        status.safe_to_replace = false;
        if !entry.starts_with("!! ") {
            status.clean = false;
        }
    }
    Ok(status)
}

/// The branch a default-branch clone checked out (the remote HEAD).
pub(super) fn current_branch(
    context: &CloneContext<'_>,
    directory: &Path,
) -> Result<String, CloneError> {
    let output = run(
        context,
        scoped(directory, &["symbolic-ref", "--quiet", "--short", "HEAD"]),
        head_timeout(context),
        "read checkout branch",
        None,
    )?;
    let branch = output.stdout.trim().to_owned();
    if branch.is_empty() {
        return Err(CloneError::new(
            "gitFailed",
            "Git did not report the checked-out default branch.",
        ));
    }
    Ok(branch)
}

/// Check out `branch` (the remote default when `None`), a tag, or a full
/// commit SHA with `depth` commits of history, limited to `sparse_paths`.
pub(super) fn checkout(
    context: &CloneContext<'_>,
    repository_url: &str,
    branch: Option<&str>,
    sparse_paths: Option<&[String]>,
    depth: u64,
    target: &Path,
) -> Result<(), CloneError> {
    match branch {
        Some(commit) if is_commit(commit) => {
            checkout_commit(context, repository_url, commit, sparse_paths, depth, target)
        }
        _ => checkout_branch(context, repository_url, branch, sparse_paths, depth, target),
    }
}

/// `git clone` of one branch. Without `branch`, git follows the remote HEAD,
/// so no API call is needed to learn the default branch.
fn checkout_branch(
    context: &CloneContext<'_>,
    repository_url: &str,
    branch: Option<&str>,
    sparse_paths: Option<&[String]>,
    depth: u64,
    target: &Path,
) -> Result<(), CloneError> {
    let mut args: Vec<OsString> = vec!["clone".into()];
    if sparse_paths.is_some() {
        args.extend(["--filter", "blob:none", "--sparse"].map(OsString::from));
    }
    args.extend([
        OsString::from("--depth"),
        OsString::from(depth.to_string()),
        OsString::from("--single-branch"),
    ]);
    if let Some(branch) = branch {
        args.extend([OsString::from("--branch"), OsString::from(branch)]);
    }
    args.extend([
        OsString::from("--"),
        OsString::from(repository_url),
        target.as_os_str().to_owned(),
    ]);
    run(
        context,
        args,
        clone_timeout(context),
        if sparse_paths.is_some() {
            "sparse clone"
        } else {
            "full clone"
        },
        Some(repository_url),
    )?;
    if let Some(paths) = sparse_paths {
        run(
            context,
            sparse_set_args(context, target, "HEAD", paths),
            sparse_timeout(context),
            "set sparse checkout paths",
            Some(repository_url),
        )?;
    }
    Ok(())
}

/// `sparse-checkout set` arguments for `sparse_paths` at `rev`. Cone mode
/// always includes every file beside each cone directory, so when any path is
/// a file every path becomes an exact anchored non-cone pattern (a directory
/// as `/dir/`); directories alone (and paths that do not resolve, left for
/// the caller's not-found check) keep cone mode.
fn sparse_set_args(
    context: &CloneContext<'_>,
    target: &Path,
    rev: &str,
    sparse_paths: &[String],
) -> Vec<OsString> {
    let paths = sparse_paths
        .iter()
        .map(|path| path.trim_end_matches('/'))
        .collect::<Vec<_>>();
    // ls-tree reads tree objects only, so it never lazily fetches a blob;
    // `-z` keeps unusual paths unquoted.
    let mut ls_tree = scoped(target, &["ls-tree", "-z", rev, "--"]);
    ls_tree.extend(paths.iter().map(OsString::from));
    let files = run(
        context,
        ls_tree,
        sparse_timeout(context),
        "resolve sparse path type",
        None,
    )
    .map(|output| {
        output
            .stdout
            .split('\0')
            .filter_map(|line| {
                let (meta, entry) = line.split_once('\t')?;
                (meta.split_whitespace().nth(1) == Some("blob")).then(|| entry.to_owned())
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    if paths
        .iter()
        .any(|path| files.iter().any(|file| file == path))
    {
        let mut args = scoped(target, &["sparse-checkout", "set", "--no-cone", "--"]);
        args.extend(paths.iter().map(|path| {
            let mut pattern = file_pattern(path);
            if !files.iter().any(|file| file == path) {
                pattern.push('/');
            }
            OsString::from(pattern)
        }));
        args
    } else {
        let mut args = scoped(
            target,
            &["sparse-checkout", "set", "--cone", "--skip-checks", "--"],
        );
        args.extend(sparse_paths.iter().map(OsString::from));
        args
    }
}

/// Anchored gitignore-style pattern matching exactly one repo-relative file.
fn file_pattern(path: &str) -> String {
    let mut pattern = String::from("/");
    for character in path.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | ']' | '!' | '#') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

fn checkout_commit(
    context: &CloneContext<'_>,
    repository_url: &str,
    commit: &str,
    sparse_paths: Option<&[String]>,
    depth: u64,
    target: &Path,
) -> Result<(), CloneError> {
    run(
        context,
        vec!["init".into(), "--".into(), target.as_os_str().to_owned()],
        clone_timeout(context),
        "initialize commit checkout",
        None,
    )?;
    run(
        context,
        scoped(target, &["remote", "add", "origin", repository_url]),
        clone_timeout(context),
        "configure commit remote",
        Some(repository_url),
    )?;
    if sparse_paths.is_some() {
        run(
            context,
            scoped(target, &["config", "remote.origin.promisor", "true"]),
            sparse_timeout(context),
            "configure sparse promisor",
            Some(repository_url),
        )?;
        run(
            context,
            scoped(
                target,
                &["config", "remote.origin.partialclonefilter", "blob:none"],
            ),
            sparse_timeout(context),
            "configure sparse filter",
            Some(repository_url),
        )?;
    }
    let depth = depth.to_string();
    let mut fetch = scoped(target, &["fetch", "--depth", &depth]);
    if sparse_paths.is_some() {
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
        clone_timeout(context),
        "fetch requested commit",
        Some(repository_url),
    )?;
    if let Some(paths) = sparse_paths {
        run(
            context,
            sparse_set_args(context, target, "FETCH_HEAD", paths),
            sparse_timeout(context),
            "set sparse commit paths",
            Some(repository_url),
        )?;
    }
    run(
        context,
        scoped(target, &["checkout", "--detach", "FETCH_HEAD", "--"]),
        clone_timeout(context),
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
            "invalidInput",
            "Repository URL could not be parsed",
        ));
    };
    let budget = GitHubBudget::global();
    let state_dir = context
        .config
        .persistent
        .then(|| context.config.cache_home.join("tmp").join("ratelimit"));
    budget
        .acquire_git_blocking(
            &LimiterKey::for_url(&url, token),
            state_dir.as_deref(),
            GIT_COOLDOWN_CAP,
            context.deadline,
            &|| context.cancellation.check().is_err(),
        )
        .map_err(|error| match error.kind {
            ProviderErrorKind::Cancelled => CloneError::new("cancelled", error.message.to_string()),
            ProviderErrorKind::Timeout => CloneError::new("timeout", error.message.to_string()),
            _ => CloneError::new(
                "rateLimited",
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
    let authorization = context
        .credential
        .map(|credential| credential.expose_secret());
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
