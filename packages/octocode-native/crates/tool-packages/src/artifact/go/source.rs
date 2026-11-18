//! Go module source refs and repository directories.
use super::super::ArtifactItem;
use super::super::http::RegistryClient;
use super::super::release_ref::CheckFailure;
use super::super::util::{commit_sha, github_slug};
use serde_json::Value;

/// The VCS ref a Go module version names: a pseudo-version's commit, or the
/// release tag (prefixed by the module's directory inside its repository;
/// a major-version suffix is never part of that prefix).
pub(super) fn go_source_ref(artifact: &ArtifactItem) -> Option<String> {
    if let Some(commit) = go_pseudo_commit(artifact) {
        return Some(commit);
    }
    let version = artifact.version.as_deref()?;
    let version = version.strip_suffix("+incompatible").unwrap_or(version);
    let directory = go_module_directory(artifact)?;
    Some(if directory.is_empty() {
        version.to_owned()
    } else {
        format!("{directory}/{version}")
    })
}

/// The commit a pseudo-version names (`vX.Y.Z-yyyymmddhhmmss-abcdefabcdef`,
/// also with a `.0.`/`-0.` base): no tag holds it, so it is the registry's
/// ref as written.
pub(super) fn go_pseudo_commit(artifact: &ArtifactItem) -> Option<String> {
    let version = artifact.version.as_deref()?;
    let version = version.strip_suffix("+incompatible").unwrap_or(version);
    version
        .rsplit_once('-')
        .map(|(_, tail)| tail)
        .filter(|tail| tail.len() == 12 && version.matches('-').count() >= 2)
        .and_then(|tail| commit_sha(Some(&Value::String(tail.to_owned()))))
}

/// The module path's segments below its repository, with the module path's
/// major-version suffix (`v2`) dropped: it is never part of a directory.
fn directory_segments(path: &str) -> Vec<&str> {
    let mut segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if segments.last().is_some_and(|last| {
        last.strip_prefix('v')
            .is_some_and(|major| !major.is_empty() && major.bytes().all(|b| b.is_ascii_digit()))
    }) {
        segments.pop();
    }
    segments
}

/// The module's tag prefix inside its repository, from its path under the
/// repository root: `service/s3` for `github.com/aws/aws-sdk-go-v2/service/s3`
/// (empty at the root; a major-version suffix is never part of it, though a
/// `/vN` module's files may sit in a `vN` directory below it, which
/// [`major_directory`] settles). `None` for a vanity path that does not start
/// with the repository.
pub(super) fn go_module_directory(artifact: &ArtifactItem) -> Option<String> {
    let module = artifact.module_path.as_deref()?;
    let repository = artifact.repository.as_deref()?;
    let root = repository
        .split_once("://")
        .map_or(repository, |(_, rest)| rest)
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let dir = module.strip_prefix(root)?;
    if !(dir.is_empty() || dir.starts_with('/')) {
        return None;
    }
    Some(directory_segments(dir).join("/"))
}

/// A repository or path name as comparable text.
fn name_key(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// The directory a vanity module sits in inside its repository, read off its
/// path: the segments after the last one that names the repository
/// (`google.golang.org/grpc` is the root of `grpc/grpc-go`, `go.etcd.io/etcd/client/v3`
/// is `client` in `etcd-io/etcd`). A path that never names the repository
/// places the module nowhere, so no directory is derived (`None`; the root
/// reads as `Some("")`).
pub(super) fn go_vanity_directory(artifact: &ArtifactItem) -> Option<String> {
    let module = artifact.module_path.as_deref()?;
    let (_, repo) = super::super::util::github_slug(artifact.repository.as_deref()?)?;
    let repo = name_key(&repo);
    // Go repositories are often `<name>-go` or `go-<name>`.
    let short = repo
        .strip_suffix("-go")
        .or_else(|| repo.strip_prefix("go-"))
        .unwrap_or(&repo);
    let segments = directory_segments(module);
    let at = segments.iter().rposition(|segment| {
        let key = name_key(segment);
        key == repo || key == short
    })?;
    Some(segments[at + 1..].join("/"))
}

/// The major-version suffix a module path ends in (`v2` of
/// `github.com/o/r/v2`), from 2 up: only such a module may sit in a
/// subdirectory of that name.
fn major_suffix(module: &str) -> Option<&str> {
    let last = module.rsplit('/').next()?;
    let digits = last.strip_prefix('v')?;
    let canonical = digits.bytes().all(|b| b.is_ascii_digit()) && !digits.starts_with('0');
    (canonical && digits.parse::<u64>().is_ok_and(|major| major >= 2)).then_some(last)
}

/// Where a `/vN` module's files sit at the row's ref, as the go command finds
/// them: in `<prefix>/vN` when that directory holds a `go.mod` (a published
/// version's `vN/go.mod` declares the module, or the go command would not
/// serve the version), else at the tag prefix (a major branch). `prefix` is
/// the module's tag prefix (`""` at the repository root). Read from the
/// repository's tree at the ref: `Some(Ok(directory))` when it is `vN`,
/// `Some(Err(disclosure))` when the tree could not say, `None` when the module
/// sits at its prefix or the row has no `/vN` module, GitHub ref or access.
pub(super) async fn major_directory(
    artifact: &ArtifactItem,
    prefix: Option<&str>,
    client: &RegistryClient<'_>,
) -> Option<Result<String, String>> {
    let major = artifact.module_path.as_deref().and_then(major_suffix)?;
    let prefix = prefix?;
    let reference = artifact.source_ref.as_deref()?;
    let (owner, repo) = github_slug(artifact.repository.as_deref()?)?;
    let tags = client.tags?;
    let directory = match prefix {
        "" => major.to_owned(),
        prefix => format!("{prefix}/{major}"),
    };
    let go_mod = format!("{directory}/go.mod");
    let cause = match tags.tree_files(&owner, &repo, reference).await {
        Ok(tree) if tree.paths.contains(&go_mod) => return Some(Ok(directory)),
        Ok(tree) if !tree.truncated => return None,
        Ok(_) => format!(
            "the part of the repository's tree at {reference} GitHub returns (its recursive-tree limit cut the rest) does not list {go_mod}"
        ),
        Err(CheckFailure::NotGitHub | CheckFailure::Unavailable) => return None,
        Err(failure) => format!(
            "the repository's tree at {reference} could not be read ({})",
            failure.describe()
        ),
    };
    let at = match prefix {
        "" => "the repository root".to_owned(),
        prefix => prefix.to_owned(),
    };
    Some(Err(format!(
        "{} {}: repositoryDirectory is unknown, {directory} or {at}: {cause}.",
        artifact.package_name,
        artifact.version.as_deref().unwrap_or_default()
    )))
}
