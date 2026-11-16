//! Repository paths: lexical resolution and the missing-path recovery walk.
use super::repository_not_found;
use crate::id::ToolId;
use crate::response::result::Continuation;
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::{Value, json};

/// A non-empty repository-relative path that stays below the directory it is
/// joined to and that git itself would check out: no root, drive, backslash,
/// empty, `.` or `..` segment (so no `./`, `//` or trailing `/`), and no
/// segment a filesystem resolves to a git directory ([`is_git_directory`]).
pub(crate) fn is_repo_relative(path: &str) -> bool {
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
pub(crate) fn missing_path(error: &ProviderError, path: Option<&str>) -> bool {
    error.kind == ProviderErrorKind::NotFound
        && !repository_not_found(error)
        && error.reason != Some(ProviderErrorReason::RefNotFound)
        && path.is_some_and(|path| !path.trim_matches('/').is_empty())
}

/// Where a missing path's nearest existing parts are.
pub(crate) struct PathRecovery {
    /// Deepest existing directory on the requested path (case-corrected).
    pub(crate) directory: String,
    /// The requested file itself when only its case differed.
    pub(crate) file: Option<String>,
    /// The listing the wanted name was missing from is as long as the
    /// Contents API lists ([`CONTENTS_LIMIT`]): the name may sit past the cut.
    pub(crate) truncated: bool,
}

/// The most entries the Contents API lists of one directory.
pub(crate) const CONTENTS_LIMIT: usize = 1000;

/// The recovery's parent listing is as long as the Contents API lists.
pub(crate) const LISTING_MAY_BE_CUT_HINT: &str = "The parent directory lists 1,000 entries, the most the Contents API returns: the path may exist past them.";

/// A place in a repository: its coordinates, a path (`""` for the root)
/// and a ref (the default branch when `None`). Recoveries list under it,
/// and every cross-tool lead to a repository starts at one.
pub(crate) struct RepoPath<'a> {
    pub(crate) owner: &'a str,
    pub(crate) repo: &'a str,
    pub(crate) path: &'a str,
    pub(crate) reference: Option<&'a str>,
}

impl<'a> RepoPath<'a> {
    /// The repository's root on its default branch.
    pub(crate) fn root(owner: &'a str, repo: &'a str) -> Self {
        Self {
            owner,
            repo,
            path: "",
            reference: None,
        }
    }

    /// The query row of this place: `owner`, `repo`, then `path` and `ref`
    /// when set.
    pub(crate) fn row(&self) -> Value {
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
    pub(crate) fn lead(&self, tool: ToolId) -> Continuation {
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
/// remaining segment case-insensitively ([`walk_listings`]): the missing
/// path's parent is listed first, and a level above it only while that
/// parent is missing, so a recovery reads what the path's own depth names,
/// never the commit's recursive tree (whose size follows the repository's).
pub(crate) async fn locate_path(
    provider: &GitHubProvider,
    at: &RepoPath<'_>,
    context: &RequestContext,
) -> Option<PathRecovery> {
    let segments: Vec<&str> = at.path.split('/').filter(|part| !part.is_empty()).collect();
    let reference = provider
        .resolve_reference(at.owner, at.repo, at.reference, context)
        .await
        .ok()?;
    walk_listings(provider, at, &segments, &reference, context).await
}

/// [`locate_path`] by directory listings, one per level walked.
async fn walk_listings(
    provider: &GitHubProvider,
    at: &RepoPath<'_>,
    segments: &[&str],
    reference: &str,
    context: &RequestContext,
) -> Option<PathRecovery> {
    let (owner, repo) = (at.owner, at.repo);
    let mut depth = segments.len().saturating_sub(1);
    let mut listing = loop {
        let directory = segments[..depth].join("/");
        match provider
            .repository_contents(owner, repo, &directory, reference, context)
            .await
        {
            // A file path answers one entry of itself: no directory to list.
            Ok(listing) if depth > 0 && names_a_file(&listing, &directory) => depth -= 1,
            Ok(listing) => break listing,
            Err(error) if error.kind == ProviderErrorKind::NotFound && depth > 0 => {
                depth -= 1;
            }
            Err(_) => return None,
        }
    };
    let mut directory = segments[..depth].join("/");
    let mut truncated = false;
    for (index, wanted) in segments.iter().enumerate().skip(depth) {
        let matches: Vec<_> = listing
            .entries
            .iter()
            .filter(|entry| entry.name.eq_ignore_ascii_case(wanted))
            .collect();
        let entry = match matches.as_slice() {
            [entry] => *entry,
            many => match many.iter().find(|entry| entry.name == *wanted) {
                Some(entry) => *entry,
                None => {
                    truncated = listing.raw_entry_count >= CONTENTS_LIMIT;
                    break;
                }
            },
        };
        let (kind, path) = (entry.kind.clone(), entry.path.clone());
        let last = index + 1 == segments.len();
        if last && kind == "file" {
            return Some(PathRecovery {
                directory,
                file: Some(path),
                truncated: false,
            });
        }
        if kind != "dir" {
            break;
        }
        if last {
            directory = path;
            break;
        }
        match provider
            .repository_contents(owner, repo, &path, reference, context)
            .await
        {
            Ok(next) => {
                listing = next;
                directory = path;
            }
            Err(_) => break,
        }
    }
    Some(PathRecovery {
        directory,
        file: None,
        truncated,
    })
}

/// Whether `listing` is the Contents API's answer for a file at `directory`:
/// one object that is that path and no directory.
fn names_a_file(listing: &octocode_github::ContentsListing, directory: &str) -> bool {
    matches!(listing.entries.as_slice(), [entry] if entry.path == directory && entry.kind != "dir")
}

/// The parent directory of a repository path, `.` at the root.
pub(crate) fn parent_dir(path: &str) -> String {
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
pub(crate) fn listing_path(path: &str) -> Result<String, ProviderError> {
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
pub(crate) fn tree_recovery(at: &RepoPath<'_>, max_depth: Option<i64>) -> Value {
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
