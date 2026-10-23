use super::{CloneContext, CloneError, GitRunRequest, cache, control};
use octocode_contract_types::ErrorCode;
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
    if !octocode_github::is_full_sha(&sha) {
        return Err(CloneError::new(
            ErrorCode::GitFailed,
            "Git returned an invalid checkout HEAD commit SHA.",
        ));
    }
    Ok(sha)
}

/// The entry names (files, directories and submodules) directly under `dir`
/// at the checkout's HEAD commit; `dir` is repo-relative, empty for the
/// root. Empty when `dir` is no directory of the commit. `ls-tree` reads
/// tree objects only, so a sparse checkout lists the whole commit without
/// fetching a blob.
pub(super) fn tree_names(
    context: &CloneContext<'_>,
    directory: &Path,
    dir: &str,
) -> Result<Vec<String>, CloneError> {
    let mut args = vec![
        OsString::from("-C"),
        directory.as_os_str().to_owned(),
        "ls-tree".into(),
        "-z".into(),
        "--name-only".into(),
        "HEAD".into(),
    ];
    if !dir.is_empty() {
        args.extend(["--".into(), format!("{dir}/").into()]);
    }
    let output = run(
        context,
        args,
        head_timeout(context),
        "list checkout tree",
        None,
    )?;
    Ok(output
        .stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
        .collect())
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

/// Repo-relative paths of the repository symlinks (index mode `120000`) the
/// working tree holds: [`HARDENING_ARGS`] checks each out as a plain file
/// holding the link target, and a sparse checkout holds only its scope.
pub(super) fn symlinks(
    context: &CloneContext<'_>,
    directory: &Path,
) -> Result<Vec<String>, CloneError> {
    let mut links = Vec::new();
    let request = GitRunRequest {
        args: hardened(scoped(directory, &["ls-files", "--stage", "-z"])),
        timeout: head_timeout(context),
        label: "list checkout symlinks".into(),
        authorization: None,
        authorization_url: None,
    };
    context
        .git
        .run_nul_records(&request, &control(context), &mut |line| {
            let line = String::from_utf8_lossy(line);
            if let Some((meta, path)) = line.split_once('\t')
                && meta.split_whitespace().next() == Some("120000")
                && std::fs::symlink_metadata(directory.join(path)).is_ok()
            {
                links.push(path.to_owned());
            }
            Ok(())
        })?;
    Ok(links)
}

/// `sparse-checkout set` arguments for `sparse_paths` at the fetched commit. Cone mode
/// always includes every file beside each cone directory, so when any path is
/// a file every path becomes an exact anchored non-cone pattern (a directory
/// as `/dir/`); directories alone (and paths that do not resolve, left for
/// the caller's not-found check) keep cone mode. A failed `ls-tree` fails
/// the checkout: guessing cone mode would widen a file scope to its folder.
fn sparse_set_args(
    context: &CloneContext<'_>,
    target: &Path,
    sparse_paths: &[String],
) -> Result<Vec<OsString>, CloneError> {
    let paths = sparse_paths
        .iter()
        .map(|path| path.trim_end_matches('/'))
        .collect::<Vec<_>>();
    // ls-tree reads tree objects only, so it never lazily fetches a blob;
    // `-z` keeps unusual paths unquoted.
    let mut ls_tree = scoped(target, &["ls-tree", "-z", "FETCH_HEAD", "--"]);
    ls_tree.extend(paths.iter().map(OsString::from));
    let files = run(
        context,
        ls_tree,
        sparse_timeout(context),
        "resolve sparse path type",
        None,
    )?
    .stdout
    .split('\0')
    .filter_map(|line| {
        let (meta, entry) = line.split_once('\t')?;
        (meta.split_whitespace().nth(1) == Some("blob")).then(|| entry.to_owned())
    })
    .collect::<Vec<_>>();
    let is_file = |path: &str| files.iter().any(|file| file == path);
    if !paths.iter().any(|path| is_file(path)) {
        let mut args = scoped(
            target,
            &["sparse-checkout", "set", "--cone", "--skip-checks", "--"],
        );
        args.extend(sparse_paths.iter().map(OsString::from));
        return Ok(args);
    }
    let mut args = scoped(target, &["sparse-checkout", "set", "--no-cone", "--"]);
    args.extend(paths.iter().map(|path| {
        let mut pattern = file_pattern(path);
        if !is_file(path) {
            pattern.push('/');
        }
        OsString::from(pattern)
    }));
    Ok(args)
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

/// Check out `commit` (a full SHA: `run` pins every row to one) with
/// `depth` commits of history, limited to `sparse_paths`.
pub(super) fn checkout(
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
    // `remote add` and `config` are local: no credential, no network permit.
    run(
        context,
        scoped(target, &["remote", "add", "origin", repository_url]),
        clone_timeout(context),
        "configure commit remote",
        None,
    )?;
    if sparse_paths.is_some() {
        run(
            context,
            scoped(target, &["config", "remote.origin.promisor", "true"]),
            sparse_timeout(context),
            "configure sparse promisor",
            None,
        )?;
        run(
            context,
            scoped(
                target,
                &["config", "remote.origin.partialclonefilter", "blob:none"],
            ),
            sparse_timeout(context),
            "configure sparse filter",
            None,
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
            sparse_set_args(context, target, paths)?,
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

fn hardened(args: Vec<OsString>) -> Vec<OsString> {
    HARDENING_ARGS
        .iter()
        .map(OsString::from)
        .chain(args)
        .collect()
}

/// Longest cooldown a clone waits out before failing fast.
const GIT_COOLDOWN_CAP: Duration = Duration::from_secs(10);

fn git_permit(
    context: &CloneContext<'_>,
    repository_url: &str,
    token: Option<&str>,
) -> Result<tokio::sync::OwnedSemaphorePermit, CloneError> {
    use octocode_github::{GitHubBudget, LimiterKey};
    let Ok(url) = url::Url::parse(repository_url) else {
        return Err(CloneError::new(
            ErrorCode::InvalidInput,
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
        .map_err(admission_error)
}

/// Why the shared GitHub limiter refused a git transfer: the caller's own
/// cancellation or deadline, else a secondary-limit cooldown or an open
/// circuit, which the row states as the limiter's own provider failure
/// (rate-limit facts, retryable, `next.retry`).
fn admission_error(error: octocode_github::ProviderError) -> CloneError {
    use octocode_github::ProviderErrorKind;
    match error.kind {
        ProviderErrorKind::Cancelled => {
            CloneError::new(ErrorCode::Cancelled, error.message.to_string())
        }
        ProviderErrorKind::Timeout => {
            CloneError::new(ErrorCode::Timeout, error.message.to_string())
        }
        _ => CloneError::provider(error),
    }
}

fn run(
    context: &CloneContext<'_>,
    args: Vec<OsString>,
    timeout: Duration,
    label: &str,
    authorization_url: Option<&str>,
) -> Result<super::GitOutput, CloneError> {
    let args = hardened(args);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::gh_clone_repo::CloneFailure;
    use octocode_github::{ProviderError, ProviderErrorKind, RateLimit};

    fn limited(kind: ProviderErrorKind) -> ProviderError {
        let mut error = ProviderError::new(kind, "GitHub secondary rate limit; request not sent.");
        error.rate_limit = Some(Box::new(RateLimit {
            remaining: None,
            reset_epoch_seconds: None,
            retry_after_seconds: Some(42),
            resource: None,
        }));
        error.retryable = true;
        error
    }

    /// A limiter cooldown is the limiter's provider failure, not clone text:
    /// the row keeps its rate-limit block, retryability and retry delay.
    #[test]
    fn a_limiter_rate_limit_is_the_rows_provider_failure() {
        match CloneFailure::from(admission_error(limited(ProviderErrorKind::RateLimited))) {
            CloneFailure::Provider(error) => {
                assert_eq!(error.kind, ProviderErrorKind::RateLimited);
                assert!(error.retryable);
                assert_eq!(
                    error.rate_limit.and_then(|rate| rate.retry_after_seconds),
                    Some(42)
                );
            }
            CloneFailure::Clone(error) => panic!("rate limit flattened to text: {error:?}"),
        }
    }

    /// An open circuit names its real cause (an outage), not a rate limit.
    #[test]
    fn an_open_circuit_keeps_its_cause() {
        match CloneFailure::from(admission_error(limited(ProviderErrorKind::Transport))) {
            CloneFailure::Provider(error) => assert_eq!(error.kind, ProviderErrorKind::Transport),
            CloneFailure::Clone(error) => panic!("circuit flattened to text: {error:?}"),
        }
    }

    /// The caller's own cancellation or deadline stays the clone's error.
    #[test]
    fn the_callers_own_stop_stays_a_clone_error() {
        for (kind, code) in [
            (ProviderErrorKind::Cancelled, ErrorCode::Cancelled),
            (ProviderErrorKind::Timeout, ErrorCode::Timeout),
        ] {
            match CloneFailure::from(admission_error(ProviderError::new(kind, "stopped"))) {
                CloneFailure::Clone(error) => assert_eq!(error.code, code),
                CloneFailure::Provider(error) => panic!("caller stop became provider: {error:?}"),
            }
        }
    }
}
