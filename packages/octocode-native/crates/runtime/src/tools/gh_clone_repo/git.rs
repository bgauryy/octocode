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

fn run(
    context: &CloneContext<'_>,
    args: Vec<OsString>,
    timeout: Duration,
    label: &str,
    authorization_url: Option<&str>,
) -> Result<super::GitOutput, CloneError> {
    let authorization = context.credential.map(|credential| credential.expose());
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
