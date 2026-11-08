mod admission;
mod sparse;

use super::{CloneContext, CloneError, GitRunRequest, control};
use crate::policy::discovery::is_sensitive_repository_file;
use admission::git_permit;
use octocode_contract_types::ErrorCode;
use sparse::{sparse_set_args, withhold_args};
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

/// What a reusable checkout holds besides its verified commit.
pub(super) struct Inspection {
    /// The credential-named files in scope ([`withheld_files`]).
    pub(super) listed: Vec<WithheldFile>,
    /// Repository symlinks ([`symlinks`]).
    pub(super) symlinks: Vec<String>,
}

/// The checkout at `directory` when it is exactly `expected_commit` and
/// safe to reuse ([`safe_to_replace`]'s checks), with the facts a reuse
/// reports; `None` when it differs. The five git reads (status, HEAD, local
/// refs, credential-named files, symlinks) are independent, so they run
/// together: a reuse costs the slowest read, not their sum. A failure is
/// the first of them in that order; a checkout that differs is `None`
/// whatever the reads after the check say.
pub(super) fn inspect_checkout(
    context: &CloneContext<'_>,
    directory: &Path,
    expected_commit: &str,
    scope: Option<&[String]>,
) -> Result<Option<Inspection>, CloneError> {
    let _ = joined::<()>;
    let (clean, head, refs, listed, symlinks) = (
        status_is_clean(context, directory),
        read_head(context, directory),
        no_local_refs(context, directory),
        withheld_files(context, directory, "HEAD", scope),
        symlinks(context, directory),
    );
    if !(clean? && head?.eq_ignore_ascii_case(expected_commit) && refs?) {
        return Ok(None);
    }
    Ok(Some(Inspection {
        listed: listed?,
        symlinks: symlinks?,
    }))
}

/// A scoped thread's result; its panic is the caller's panic.
fn joined<T>(result: std::thread::Result<T>) -> T {
    result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Inspect ignored files as well: they do not change the verified revision,
/// but replacing the checkout would still destroy them.
pub(super) fn safe_to_replace(
    context: &CloneContext<'_>,
    directory: &Path,
    expected_commit: &str,
) -> Result<bool, CloneError> {
    if !status_is_clean(context, directory)?
        || !read_head(context, directory)?.eq_ignore_ascii_case(expected_commit)
    {
        return Ok(false);
    }
    no_local_refs(context, directory)
}

/// No modified, untracked or ignored file in the work tree.
fn status_is_clean(context: &CloneContext<'_>, directory: &Path) -> Result<bool, CloneError> {
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
    Ok(output.stdout.split('\0').all(str::is_empty))
}

/// The checkout holds no local ref.
fn no_local_refs(context: &CloneContext<'_>, directory: &Path) -> Result<bool, CloneError> {
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

/// Repo-relative paths of the credential-named files (`policy::discovery`:
/// `.env`, key files, ...) in the tree of `rev` that fall inside `scope` (the
/// requested sparse paths; everything when `None`), each with its blob id.
/// Checkouts never write them. Tree objects only: no blob is fetched.
pub(super) fn withheld_files(
    context: &CloneContext<'_>,
    directory: &Path,
    rev: &str,
    scope: Option<&[String]>,
) -> Result<Vec<WithheldFile>, CloneError> {
    let mut withheld = Vec::new();
    let request = GitRunRequest {
        args: hardened(scoped(directory, &["ls-tree", "-r", "-z", rev])),
        timeout: head_timeout(context),
        label: "list credential-named files".into(),
        authorization: None,
        authorization_url: None,
    };
    context
        .git
        .run_nul_records(&request, &control(context), &mut |line| {
            let line = String::from_utf8_lossy(line);
            // `<mode> <type> <blob>\t<path>`; a submodule is no blob.
            let Some((meta, path)) = line.split_once('\t') else {
                return Ok(());
            };
            let mut meta = meta.split_whitespace().skip(1);
            let (Some("blob"), Some(blob)) = (meta.next(), meta.next()) else {
                return Ok(());
            };
            let in_scope = scope.is_none_or(|paths| {
                paths.iter().any(|requested| {
                    let requested = requested.trim_end_matches('/');
                    path == requested
                        || path
                            .strip_prefix(requested)
                            .is_some_and(|rest| rest.starts_with('/'))
                })
            });
            if in_scope && is_sensitive_repository_file(path) {
                withheld.push(WithheldFile {
                    path: path.to_owned(),
                    blob: blob.to_owned(),
                });
            }
            Ok(())
        })?;
    Ok(withheld)
}

/// A credential-named file of a commit: its path and its blob id.
pub(super) struct WithheldFile {
    pub(super) path: String,
    blob: String,
}

/// Paths of `files`.
pub(super) fn withheld_paths(files: Vec<WithheldFile>) -> Vec<String> {
    files.into_iter().map(|file| file.path).collect()
}

/// Whether no blob of `files` is in the object store of `directory`. The
/// walk reads the store without fetching: a missing blob of a partial clone
/// is listed as `?<id>`.
pub(super) fn blobs_absent(
    context: &CloneContext<'_>,
    directory: &Path,
    files: &[WithheldFile],
) -> Result<bool, CloneError> {
    if files.is_empty() {
        return Ok(true);
    }
    let mut args = scoped(
        directory,
        &["rev-list", "--objects", "--no-walk", "--missing=print"],
    );
    args.extend(files.iter().map(|file| OsString::from(&file.blob)));
    let output = run(
        context,
        args,
        head_timeout(context),
        "check credential blobs are not stored",
        None,
    )?;
    let missing = output
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix('?'))
        .collect::<std::collections::HashSet<_>>();
    Ok(files
        .iter()
        .all(|file| missing.contains(file.blob.as_str())))
}

/// Check out `commit` (a full SHA: `run` pins every row to one) with
/// `depth` commits of history, limited to `sparse_paths`, leaving out every
/// credential-named file in scope. Returns the files left out.
pub(super) fn checkout(
    context: &CloneContext<'_>,
    repository_url: &str,
    commit: &str,
    sparse_paths: Option<&[String]>,
    depth: u64,
    target: &Path,
) -> Result<Vec<String>, CloneError> {
    run(
        context,
        vec!["init".into(), "--".into(), target.as_os_str().to_owned()],
        clone_timeout(context),
        "initialize commit checkout",
        None,
    )?;
    // `remote add` is local: no credential, no network permit.
    run(
        context,
        scoped(target, &["remote", "add", "origin", repository_url]),
        clone_timeout(context),
        "configure commit remote",
        None,
    )?;
    // Every checkout is a partial one: blobs arrive only when the checkout
    // needs them, so a credential-named file left out of it is never fetched.
    // The fetch below carries `--filter`, and a filtered fetch registers the
    // remote as a promisor with that filter itself
    // (`remote.origin.promisor`, `remote.origin.partialclonefilter`), so no
    // separate `config` step sets them.
    let depth = depth.to_string();
    let mut fetch = scoped(target, &["fetch", "--no-tags", "--depth", &depth]);
    fetch.extend([OsString::from("--filter"), OsString::from("blob:none")]);
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
    let listed = withheld_files(context, target, "FETCH_HEAD", sparse_paths)?;
    // A remote that ignores the blob filter sends every blob, credential
    // files included: refuse rather than keep them.
    if !blobs_absent(context, target, &listed)? {
        return Err(CloneError::new(
            ErrorCode::GitFailed,
            "The remote ignored the partial-clone blob filter, so credential-named files this checkout withholds were downloaded; nothing was cloned. Use a remote that supports partial clone.",
        ));
    }
    let withheld = withheld_paths(listed);
    let scope = match sparse_paths {
        Some(paths) => Some((
            sparse_set_args(context, target, paths, &withheld)?,
            "set sparse commit paths",
        )),
        None => withhold_args(target, &withheld).map(|args| (args, "withhold credential files")),
    };
    if let Some((args, label)) = scope {
        run(
            context,
            args,
            sparse_timeout(context),
            label,
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
    Ok(withheld)
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
