//! Repository paths: lexical resolution and the missing-path recovery walk.
use super::repository_not_found;
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
    TreeRequest, TreeResponse,
};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
use serde_json::{Value, json};

/// A non-empty repository-relative path that stays below the directory it is
/// joined to and that git itself would check out: no root, drive, backslash,
/// empty, `.` or `..` segment (so no `./`, `//` or trailing `/`), and no
/// segment a filesystem resolves to a git directory ([`is_git_directory`]).
pub fn is_repo_relative(path: &str) -> bool {
    let value = std::path::Path::new(path);
    !path.trim().is_empty()
        && !path.contains('\\')
        && !value.is_absolute()
        && value
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        && path
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | "..") && !is_git_directory(segment))
}

/// Whether a path segment names `.git` on some filesystem, as git's
/// `verify_path` refuses it: case-folded, with the code points HFS+/APFS
/// ignore removed, and with NTFS's trailing dots and spaces, alternate data
/// stream (`::$INDEX_ALLOCATION`) and short name (`git~1`).
fn is_git_directory(segment: &str) -> bool {
    let visible: String = segment.chars().filter(|&c| !is_hfs_ignorable(c)).collect();
    [".git", "git~1"].iter().any(|name| {
        visible
            .split_at_checked(name.len())
            .is_some_and(|(head, tail)| {
                head.eq_ignore_ascii_case(name)
                    && tail
                        .split(':')
                        .next()
                        .is_some_and(|tail| tail.chars().all(|c| matches!(c, '.' | ' ')))
            })
    })
}

/// The code points HFS+ drops when it compares names (git's `next_hfs_char`).
fn is_hfs_ignorable(c: char) -> bool {
    matches!(
        c,
        '\u{200c}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{206a}'..='\u{206f}' | '\u{feff}'
    )
}

/// A read or listing whose path (not repository or ref) did not resolve.
pub fn missing_path(error: &ProviderError, path: Option<&str>) -> bool {
    error.kind == ProviderErrorKind::NotFound
        && !repository_not_found(error)
        && error.reason != Some(ProviderErrorReason::RefNotFound)
        && path.is_some_and(|path| !path.trim_matches('/').is_empty())
}

/// Where a missing path's nearest existing parts are.
pub struct PathRecovery {
    /// Deepest existing directory on the requested path (case-corrected).
    pub directory: String,
    /// The requested file itself when only its case differed.
    pub file: Option<String>,
    /// The folder tree the wanted name was missing from is one GitHub
    /// truncated: the name may sit past the cut.
    pub truncated: bool,
}

/// The most entries the Contents API lists of one directory.
pub const CONTENTS_LIMIT: usize = 1000;

/// The recovery's parent folder tree is one GitHub truncated.
pub const LISTING_MAY_BE_CUT_HINT: &str =
    "GitHub truncated the parent directory's tree: the path may exist past the entries it lists.";

/// The git file mode of a symlink: a tree lists a link as a `blob` of it.
const SYMLINK_MODE: &str = "120000";

/// A place in a repository: its coordinates, a path (`""` for the root)
/// and a ref (the default branch when `None`). Recoveries list under it,
/// and every cross-tool lead to a repository starts at one.
pub struct RepoPath<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub path: &'a str,
    pub reference: Option<&'a str>,
}

impl RepoPath<'_> {
    /// The missing-path message both file tools state, naming the ref the
    /// caller asked for and, when that named a branch or tag, the commit it
    /// resolved to (this place's ref, which the recovery leads pin). A full
    /// SHA asked is named by its short form: the leads carry it whole. A 404
    /// at one ref says nothing of another, so an asked ref is always named.
    pub fn not_found_message(&self, asked: Option<&str>) -> String {
        let (owner, repo, path) = (self.owner, self.repo, self.path);
        let Some(asked) = asked.filter(|value| !value.is_empty()) else {
            return format!("Path not found in {owner}/{repo}: {path}");
        };
        let short = |sha: &str| sha.get(..7).unwrap_or(sha).to_owned();
        match self.reference.filter(|commit| *commit != asked) {
            Some(commit) => {
                let short = short(commit);
                format!("Path not found in {owner}/{repo} at {asked} ({short}): {path}")
            }
            None if octocode_github::is_full_sha(asked) => {
                let short = short(asked);
                format!("Path not found in {owner}/{repo} at {short}: {path}")
            }
            None => format!("Path not found in {owner}/{repo} at {asked}: {path}"),
        }
    }
}

impl<'a> RepoPath<'a> {
    /// The repository's root on its default branch.
    pub fn root(owner: &'a str, repo: &'a str) -> Self {
        Self {
            owner,
            repo,
            path: "",
            reference: None,
        }
    }

    /// The query row of this place: `owner`, `repo`, then `path` and `ref`
    /// when set.
    pub fn row(&self) -> Value {
        let mut row = json!({"owner": self.owner, "repo": self.repo});
        if !self.path.is_empty() {
            row["path"] = json!(self.path);
        }
        if let Some(reference) = self.reference {
            row["ref"] = json!(reference);
        }
        row
    }

    /// A lead to `tool` (ghStructure, ghCloneRepo) that starts here.
    pub fn lead(&self, tool: ToolId) -> Continuation {
        let mut row = self.row();
        match tool {
            ToolId::GhStructure => row["operation"] = json!("tree"),
            ToolId::GhCloneRepo => {
                if let Some(path) = row.as_object_mut().and_then(|r| r.remove("path")) {
                    row["paths"] = json!([path]);
                }
            }
            _ => {}
        }
        Continuation::related(tool, row)
    }
}

/// Walk up to the nearest directory that exists, then back down matching each
/// remaining segment case-insensitively ([`walk_trees`]): the missing path's
/// parent is read first, and a level above it only while that parent is no
/// directory, so a recovery reads what the path's own depth names, never the
/// commit's recursive tree (whose size follows the repository's).
pub async fn locate_path(
    provider: &GitHubProvider,
    at: &RepoPath<'_>,
    context: &RequestContext,
) -> Option<PathRecovery> {
    let segments: Vec<&str> = at.path.split('/').filter(|part| !part.is_empty()).collect();
    let commit = provider
        .resolve_reference(at.owner, at.repo, at.reference, context)
        .await
        .ok()?;
    walk_trees(provider, at, &segments, &commit, context).await
}

/// [`locate_path`] by folder git trees, one per level walked. A folder's own
/// tree (`<commit>:<folder>`, one level) types each entry by its mode and
/// holds no file's bytes, so no Contents read is sent: GitHub answers one of
/// a link to a file with the target's bytes under the link's name, and only
/// a tree tells such a link from a file or a folder. A link is neither a file
/// to offer nor a folder to walk. With no folder tree answering up to the
/// root, nothing is located.
async fn walk_trees(
    provider: &GitHubProvider,
    at: &RepoPath<'_>,
    segments: &[&str],
    commit: &str,
    context: &RequestContext,
) -> Option<PathRecovery> {
    let mut depth = segments.len().saturating_sub(1);
    let mut tree = loop {
        match folder_tree(provider, at, commit, &segments[..depth].join("/"), context).await {
            Ok(Some(tree)) => break tree,
            Ok(None) if depth > 0 => depth -= 1,
            Ok(None) | Err(_) => return None,
        }
    };
    let mut directory = segments[..depth].join("/");
    let mut truncated = false;
    for (index, wanted) in segments.iter().enumerate().skip(depth) {
        let matches: Vec<_> = tree
            .tree
            .iter()
            .filter(|entry| entry.path.eq_ignore_ascii_case(wanted))
            .collect();
        let entry = match matches.as_slice() {
            [entry] => *entry,
            many => match many.iter().find(|entry| entry.path == *wanted) {
                Some(entry) => *entry,
                None => {
                    truncated = tree.truncated;
                    break;
                }
            },
        };
        let path = if directory.is_empty() {
            entry.path.clone()
        } else {
            format!("{directory}/{}", entry.path)
        };
        let last = index + 1 == segments.len();
        let link = entry.mode.as_deref() == Some(SYMLINK_MODE);
        if last && entry.kind == "blob" && !link {
            return Some(PathRecovery {
                directory,
                file: Some(path),
                truncated: false,
            });
        }
        if entry.kind != "tree" {
            break;
        }
        if last {
            directory = path;
            break;
        }
        match folder_tree(provider, at, commit, &path, context).await {
            Ok(Some(next)) => {
                tree = next;
                directory = path;
            }
            Ok(None) | Err(_) => break,
        }
    }
    Some(PathRecovery {
        directory,
        file: None,
        truncated,
    })
}

/// The git tree of `folder` (`""` is the root) at `commit`, one level deep.
/// `None` when GitHub answers that it is no folder there: a 404 (absent, or
/// below a link, a file or a submodule) or a 422 (a file or a link).
async fn folder_tree(
    provider: &GitHubProvider,
    at: &RepoPath<'_>,
    commit: &str,
    folder: &str,
    context: &RequestContext,
) -> Result<Option<TreeResponse>, ProviderError> {
    let reference = if folder.is_empty() {
        commit.to_owned()
    } else {
        format!("{commit}:{folder}")
    };
    let request = TreeRequest {
        owner: at.owner.to_owned(),
        repo: at.repo.to_owned(),
        reference,
        recursive: false,
    };
    match provider.transport.get_tree(&request, context).await {
        Ok(tree) => Ok(Some(tree)),
        Err(error) if matches!(error.status, Some(404 | 422)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// The parent directory of a repository path, `.` at the root.
pub fn parent_dir(path: &str) -> String {
    let normal = repo_path(path);
    std::path::Path::new(&normal)
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| ".".into())
}

/// `path`'s segments with empty, `.` and `..` ones resolved lexically, and
/// whether a `..` climbed above the repository root (and was dropped).
fn resolved_segments(path: &str) -> (Vec<&str>, bool) {
    let mut segments: Vec<&str> = Vec::new();
    let mut escaped = false;
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => escaped |= segments.pop().is_none(),
            segment => segments.push(segment),
        }
    }
    (segments, escaped)
}

/// A repository path with its `.` and `..` segments resolved lexically and
/// clamped at the repository root (a tree has nothing above it); `.` for
/// the root. A recovery lead never repeats a traversal the caller sent.
pub(crate) fn repo_path(path: &str) -> String {
    let (segments, _) = resolved_segments(path);
    if segments.is_empty() {
        ".".into()
    } else {
        segments.join("/")
    }
}

/// A requested repository path (a listed directory, a read file) as the
/// repository names it: `.`, `..`, repeated and edge slashes resolved
/// lexically, `""` for the root. A `..` that climbs above the root names
/// nothing a repository holds: invalid input.
pub fn listing_path(path: &str) -> Result<String, ProviderError> {
    match resolved_segments(path) {
        (_, true) => Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!("Path \"{path}\" climbs above the repository root."),
        )),
        (segments, false) => Ok(segments.join("/")),
    }
}

/// A ghStructure listing of `path` at the caller's ref (and depth, when the
/// caller set one): a fresh query from page 1, not a page of the failed call.
/// `approximate` marks a listing of a guessed directory.
pub fn tree_recovery(at: &RepoPath<'_>, max_depth: Option<i64>) -> Value {
    let mut tree_query =
        json!({"operation":"tree","owner": at.owner, "repo": at.repo, "path": repo_path(at.path)});
    if let Some(reference) = at.reference {
        tree_query["ref"] = json!(reference);
    }
    if let Some(depth) = max_depth {
        tree_query["maxDepth"] = json!(depth);
    }
    let listing = Continuation::related(ToolId::GhStructure, tree_query);
    listing.build()
}

#[cfg(test)]
mod tests;
