//! Go module source refs and repository directories.
use super::super::ArtifactItem;
use super::super::util::commit_sha;
use serde_json::Value;

/// The VCS ref a Go module version names: a pseudo-version's commit, or the
/// release tag (prefixed by the module's directory inside its repository;
/// a major-version suffix is never part of that prefix).
pub(super) fn go_source_ref(artifact: &ArtifactItem) -> Option<String> {
    let version = artifact.version.as_deref()?;
    let version = version.strip_suffix("+incompatible").unwrap_or(version);
    // vX.Y.Z-yyyymmddhhmmss-abcdefabcdef (also with a .0./-0. base).
    if let Some(commit) = version
        .rsplit_once('-')
        .map(|(_, tail)| tail)
        .filter(|tail| tail.len() == 12 && version.matches('-').count() >= 2)
        .and_then(|tail| commit_sha(Some(&Value::String(tail.to_owned()))))
    {
        return Some(commit);
    }
    let directory = go_module_directory(artifact)?;
    Some(if directory.is_empty() {
        version.to_owned()
    } else {
        format!("{directory}/{version}")
    })
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

/// The module's directory inside its repository, from its path under the
/// repository root: `service/s3` for `github.com/aws/aws-sdk-go-v2/service/s3`
/// (empty at the root; a major-version suffix is never part of it). `None`
/// for a vanity path that does not start with the repository.
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
