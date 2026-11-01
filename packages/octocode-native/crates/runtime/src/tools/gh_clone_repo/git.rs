mod admission;
mod sparse;

use super::{CloneContext, CloneError, GitRunRequest, control};
use admission::git_permit;
use octocode_contract_types::ErrorCode;
use sparse::sparse_set_args;
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

/// Inspect ignored files as well: they do not change the verified revision,
/// but replacing the checkout would still destroy them.
pub(super) fn safe_to_replace(
    context: &CloneContext<'_>,
    directory: &Path,
    expected_commit: &str,
) -> Result<bool, CloneError> {
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
        "check checkout cleanliness",
        None,
    )?;
    if !output.stdout.split('\0').all(str::is_empty)
        || !read_head(context, directory)?.eq_ignore_ascii_case(expected_commit)
    {
        return Ok(false);
    }
    let refs = run(
        context,
        scoped(directory, &["for-each-ref", "--format=%(refname)"]),
        head_timeout(context),
        "check local checkout refs",
        None,
    )?;
    Ok(refs.stdout.trim().is_empty())
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
    let mut fetch = scoped(target, &["fetch", "--no-tags", "--depth", &depth]);
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
        .map(octocode_github::ResolvedCredential::expose_secret);
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
mod tests;
