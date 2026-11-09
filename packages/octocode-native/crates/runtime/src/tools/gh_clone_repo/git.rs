mod admission;
mod sparse;

use super::{CloneContext, CloneError, GitRunRequest, control, storage};
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
/// whatever the reads after the check say. A checkout git cannot read at
/// all (its store is gone) differs too: it is not reusable.
pub(super) fn inspect_checkout(
    context: &CloneContext<'_>,
    directory: &Path,
    expected_commit: &str,
    scope: Option<&[String]>,
) -> Result<Option<Inspection>, CloneError> {
    let (clean, head, refs, listed, symlinks) = std::thread::scope(|threads| {
        let clean = threads.spawn(|| status_is_clean(context, directory));
        let head = threads.spawn(|| read_head(context, directory));
        let refs = threads.spawn(|| no_local_refs(context, directory));
        let listed = threads.spawn(|| withheld_files(context, directory, "HEAD", scope));
        let symlinks = threads.spawn(|| symlinks(context, directory));
        (
            joined(clean.join()),
            joined(head.join()),
            joined(refs.join()),
            joined(listed.join()),
            joined(symlinks.join()),
        )
    });
    let clean = unreadable_is(clean, false)?;
    let head = unreadable_is(head, String::new())?;
    let refs = unreadable_is(refs, false)?;
    if !(clean && head.eq_ignore_ascii_case(expected_commit) && refs) {
        return Ok(None);
    }
    Ok(Some(Inspection {
        listed: listed?,
        symlinks: symlinks?,
    }))
}

/// `value`, or `otherwise` when git could not read the checkout at all
/// (`gitFailed`: a worktree whose store is gone, a broken repository). Any
/// other failure, a timeout or a cancellation, stays an error.
fn unreadable_is<T>(value: Result<T, CloneError>, otherwise: T) -> Result<T, CloneError> {
    match value {
        Err(error) if error.code == ErrorCode::GitFailed => Ok(otherwise),
        other => other,
    }
}

/// A scoped thread's result; its panic is the caller's panic.
fn joined<T>(result: std::thread::Result<T>) -> T {
    result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Inspect ignored files as well: they do not change the verified revision,
/// but replacing the checkout would still destroy them. A directory git
/// cannot read (a worktree whose store is gone) may hold the user's files:
/// it is not safe either.
pub(super) fn safe_to_replace(
    context: &CloneContext<'_>,
    directory: &Path,
    expected_commit: &str,
) -> Result<bool, CloneError> {
    unreadable_is(
        (|| {
            if !status_is_clean(context, directory)?
                || !read_head(context, directory)?.eq_ignore_ascii_case(expected_commit)
            {
                return Ok(false);
            }
            no_local_refs(context, directory)
        })(),
        false,
    )
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

/// Check out `commit` (a full SHA: `run` pins every row to one) as the linked
/// worktree `stage` of the shared partial `store`, with `depth` commits of
/// history, limited to `sparse_paths`, leaving out every credential-named
/// file in scope. Returns the files left out, or `None` when `store` already
/// held a credential-named blob it did not just fetch (a rule set that
/// withheld less, or an earlier remote that ignored the filter): the caller
/// makes a clean store instead.
pub(super) fn checkout(
    context: &CloneContext<'_>,
    repository_url: &str,
    store: &Path,
    commit: &str,
    sparse_paths: Option<&[String]>,
    depth: u64,
    stage: &Path,
) -> Result<Option<Vec<String>>, CloneError> {
    let created = ensure_store(context, store, repository_url)?;
    let Some(withheld) = ensure_commit(
        context,
        store,
        &Fetch {
            repository_url,
            commit,
            sparse_paths,
            depth,
            created,
        },
    )?
    else {
        return Ok(None);
    };
    add_worktree(context, store, commit, stage)?;
    let scope = match sparse_paths {
        Some(paths) => Some((
            sparse_set_args(context, stage, paths, &withheld)?,
            "set sparse commit paths",
        )),
        None => withhold_args(stage, &withheld).map(|args| (args, "withhold credential files")),
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
    // The work tree fills in from the store; a blob it lacks is fetched from
    // the store's promisor remote, which is why this step carries the
    // credential.
    run(
        context,
        scoped(stage, &["checkout", "--detach", commit, "--"]),
        clone_timeout(context),
        "check out requested commit",
        Some(repository_url),
    )?;
    Ok(Some(withheld))
}

/// The shared store at `store`: a bare repository whose every checkout is a
/// linked worktree, with the `origin` the row cloned from. Its own `HEAD`
/// and refs stay empty: a fetch names a commit, a worktree's `HEAD` keeps it
/// reachable, and every ref is somebody's local work.
fn ensure_store(
    context: &CloneContext<'_>,
    store: &Path,
    repository_url: &str,
) -> Result<bool, CloneError> {
    let created = !storage::is_ready(store);
    if created {
        init_store(context, store)?;
    }
    // The URL a lazy blob fetch of any worktree goes to, and the one askpass
    // answers for: this row's, whatever spelling an earlier row used.
    run(
        context,
        scoped(store, &["config", "remote.origin.url", repository_url]),
        head_timeout(context),
        "configure store remote",
        None,
    )?;
    // A checkout directory the user deleted leaves its administration behind.
    prune_worktrees(context, store)?;
    Ok(created)
}

/// Make `store`, idempotently: an interrupted init is finished by the next
/// one, and the ready marker is written last.
fn init_store(context: &CloneContext<'_>, store: &Path) -> Result<(), CloneError> {
    storage::create_parent(store)?;
    run(
        context,
        vec![
            "init".into(),
            "--bare".into(),
            "--".into(),
            store.as_os_str().to_owned(),
        ],
        clone_timeout(context),
        "initialize shared store",
        None,
    )?;
    let configure = |args: &[&str]| {
        run(
            context,
            scoped(store, args),
            head_timeout(context),
            "configure shared store",
            None,
        )
    };
    // Every checkout reads repository symlinks as plain files; garbage
    // collection and maintenance stay off, as only worktrees keep the
    // commits reachable and a fetch is never followed by a ref.
    for (key, value) in [
        ("core.repositoryformatversion", "1"),
        ("extensions.worktreeConfig", "true"),
        ("core.symlinks", "false"),
        ("gc.auto", "0"),
        ("maintenance.auto", "false"),
    ] {
        configure(&["config", key, value])?;
    }
    // With per-worktree config, `core.bare=true` in the shared config would
    // make every linked worktree read as bare: the store alone says so.
    let shared = configure(&["config", "--local", "--list"])?;
    if shared
        .stdout
        .lines()
        .any(|line| line.starts_with("core.bare="))
    {
        configure(&["config", "--worktree", "core.bare", "true"])?;
        configure(&["config", "--unset", "core.bare"])?;
    }
    storage::mark_ready(store)
}

/// What a row asks of its store ([`ensure_commit`]).
#[derive(Clone, Copy)]
struct Fetch<'a> {
    repository_url: &'a str,
    commit: &'a str,
    sparse_paths: Option<&'a [String]>,
    depth: u64,
    /// The store did not exist before this row.
    created: bool,
}

/// Fetch `commit` into `store` unless it already holds it, then check that
/// no credential-named blob in scope is stored. Returns the credential-named
/// files in scope, which a checkout leaves out.
///
/// A fetch with `--depth` deepens or shortens history shared with other tips,
/// so only a depth-1 store is shared: there a present commit is skipped, and
/// a fetch only adds a one-commit boundary. A deeper store belongs to one
/// commit and depth; it is skipped when it already holds that many commits.
///
/// `None` when a store that existed before this call holds a credential-named
/// blob in scope (the caller makes a clean one); a store `created` by this
/// call can only have it from a remote that ignored the filter.
fn ensure_commit(
    context: &CloneContext<'_>,
    store: &Path,
    request: &Fetch<'_>,
) -> Result<Option<Vec<String>>, CloneError> {
    let Fetch {
        repository_url,
        commit,
        sparse_paths,
        depth,
        created,
    } = *request;
    let fetching = !holds_history(context, store, commit, depth)?;
    if fetching {
        // Every checkout is a partial one: blobs arrive only when a checkout
        // needs them, so a credential-named file left out is never fetched.
        // The fetch carries `--filter`, and a filtered fetch registers the
        // remote as a promisor with that filter itself
        // (`remote.origin.promisor`, `remote.origin.partialclonefilter`).
        let depth = depth.to_string();
        let mut fetch = scoped(store, &["fetch", "--no-tags", "--depth", &depth]);
        fetch.extend([OsString::from("--filter"), OsString::from("blob:none")]);
        fetch.extend(["--".into(), "origin".into(), OsString::from(commit)]);
        run(
            context,
            fetch,
            clone_timeout(context),
            "fetch requested commit",
            Some(repository_url),
        )?;
    }
    let listed = withheld_files(context, store, commit, sparse_paths)?;
    // A remote that ignores the blob filter sends every blob, credential
    // files included: refuse rather than keep them.
    if !blobs_absent(context, store, &listed)? {
        if !created {
            return Ok(None);
        }
        return Err(CloneError::new(
            ErrorCode::GitFailed,
            "The remote ignored the partial-clone blob filter, so credential-named files this checkout withholds were downloaded; nothing was cloned. Use a remote that supports partial clone.",
        ));
    }
    Ok(Some(withheld_paths(listed)))
}

/// Whether `store` already holds `commit` with the history `depth` asks for.
fn holds_history(
    context: &CloneContext<'_>,
    store: &Path,
    commit: &str,
    depth: u64,
) -> Result<bool, CloneError> {
    // `--missing` keeps a walk from fetching what the store lacks.
    if depth == 1 {
        let listed = run(
            context,
            scoped(store, &["rev-list", "--no-walk", "--missing=print", commit]),
            head_timeout(context),
            "check commit in store",
            None,
        )?;
        return Ok(listed
            .stdout
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case(commit)));
    }
    // A history shorter than `depth` ends at the root commit: complete, not
    // missing. `--missing=print` marks each absent commit with `?`.
    let count = depth.to_string();
    let walked = run(
        context,
        scoped(
            store,
            &["rev-list", "--max-count", &count, "--missing=print", commit],
        ),
        head_timeout(context),
        "check commit in store",
        None,
    )?;
    let mut lines = walked.stdout.lines().map(str::trim);
    Ok(lines
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case(commit))
        && lines.all(|line| !line.starts_with('?')))
}

/// A new linked worktree of `store` at `stage`, detached at `commit` and
/// empty (local: no blob is read).
fn add_worktree(
    context: &CloneContext<'_>,
    store: &Path,
    commit: &str,
    stage: &Path,
) -> Result<(), CloneError> {
    let mut args = scoped(
        store,
        &["worktree", "add", "--no-checkout", "--detach", "--"],
    );
    args.extend([stage.as_os_str().to_owned(), OsString::from(commit)]);
    run(context, args, sparse_timeout(context), "add worktree", None)?;
    Ok(())
}

/// Move the worktree `from` of `store` to `to`; its administration, sparse
/// scope and index follow.
pub(super) fn move_worktree(
    context: &CloneContext<'_>,
    store: &Path,
    from: &Path,
    to: &Path,
) -> Result<(), CloneError> {
    let mut args = scoped(store, &["worktree", "move"]);
    args.extend([from.as_os_str().to_owned(), to.as_os_str().to_owned()]);
    run(
        context,
        args,
        sparse_timeout(context),
        "move worktree",
        None,
    )?;
    Ok(())
}

/// Forget the worktrees of `store` whose directories are gone.
pub(super) fn prune_worktrees(context: &CloneContext<'_>, store: &Path) -> Result<(), CloneError> {
    run(
        context,
        scoped(store, &["worktree", "prune"]),
        head_timeout(context),
        "prune removed worktrees",
        None,
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
