//! Git tree and Contents entries as listing entries, and the typed failures of a path that is not a listable directory.
use super::*;

/// A ref that did not resolve names the ref, like ghGetFileContent does.
pub(in crate::tools::gh_structure) fn missing_ref(
    error: ProviderError,
    owner: &str,
    repo: &str,
    reference: &str,
) -> ProviderError {
    if error.reason != Some(ProviderErrorReason::RefNotFound) {
        return error;
    }
    let mut missing = ProviderError::new(
        ProviderErrorKind::NotFound,
        format!("Branch, tag, or SHA not found for {owner}/{repo}: \"{reference}\""),
    )
    .with_reason(ProviderErrorReason::RefNotFound);
    missing.status = error.status;
    missing.request_id = error.request_id;
    missing
}

pub(in crate::tools::gh_structure) fn root_is_directory(
    tree: &octocode_github::TreeResponse,
    root: &str,
) -> Result<(), ProviderError> {
    if root.is_empty() {
        return Ok(());
    }
    match tree.tree.iter().find(|entry| entry.path == root) {
        Some(entry) if entry.kind != "tree" => Err(not_a_directory(root, git_kind(entry))),
        Some(_) => Ok(()),
        // A complete tree without the root: the path does not exist at this
        // commit, as the contents API would answer (404).
        None => Err(ProviderError::new(
            ProviderErrorKind::NotFound,
            format!("Path \"{root}\" not found"),
        )),
    }
}

/// A tree listing of a path that is not a directory, typed for its repair: a
/// file or a symlink ([`ProviderErrorReason::PathIsFile`]) is read (a read
/// follows a symlink to its file), and a submodule
/// ([`ProviderErrorReason::Submodule`]) leads to its own repository.
pub(in crate::tools::gh_structure) fn not_a_directory(path: &str, kind: &str) -> ProviderError {
    let (kind, reason) = match kind {
        "blob" | "file" => ("a file", Some(ProviderErrorReason::PathIsFile)),
        "symlink" => ("a symlink", Some(ProviderErrorReason::PathIsFile)),
        "commit" | "submodule" => ("a git submodule", Some(ProviderErrorReason::Submodule)),
        _ => ("an unsupported entry", None),
    };
    let error = ProviderError::new(
        ProviderErrorKind::Validation,
        format!("Path \"{path}\" is {kind}, not a directory"),
    );
    match reason {
        Some(reason) => error.with_reason(reason),
        None => error,
    }
}

pub(in crate::tools::gh_structure) fn traversal_from_git_tree(
    tree: octocode_github::TreeResponse,
    listing: &Listing<'_>,
) -> Traversal {
    let (root, max_depth) = (listing.root, listing.max_depth);
    let prefix = if root.is_empty() {
        String::new()
    } else {
        format!("{root}/")
    };
    let mut traversal = Traversal::default();
    for entry in tree.tree {
        let relative = if root.is_empty() {
            entry.path.clone()
        } else if entry.path == root {
            continue;
        } else if let Some(stripped) = entry.path.strip_prefix(&prefix) {
            stripped.to_owned()
        } else {
            continue;
        };
        let depth = relative.split('/').filter(|part| !part.is_empty()).count();
        if depth == 0 || depth > max_depth {
            continue;
        }
        let path = if root.is_empty() {
            relative
        } else {
            format!("{root}/{relative}")
        };
        let kind = match git_kind(&entry) {
            "blob" => EntryKind::File,
            "tree" => EntryKind::Dir,
            other => {
                note_unlisted(&mut traversal, &path, other);
                continue;
            }
        };
        traversal.entries.push(TreeEntry {
            path,
            kind,
            size: entry.size,
        });
    }
    traversal
}

/// A git tree entry's type, with a `blob` of the symlink mode (`120000`)
/// typed `symlink`, as the Contents API types it: a link has no listable
/// content and is never shown as a file.
pub(super) fn git_kind(entry: &octocode_github::TreeEntry) -> &str {
    if entry.kind == "blob" && entry.mode.as_deref() == Some("120000") {
        "symlink"
    } else {
        &entry.kind
    }
}

/// A symlink or submodule has no listable content: name it by its
/// repository-relative `path` instead of dropping it silently.
pub(super) fn note_unlisted(traversal: &mut Traversal, path: &str, kind: &str) {
    let label = match kind {
        "symlink" => "symlink",
        "submodule" | "commit" => "submodule",
        _ => return,
    };
    traversal.omitted.insert(format!("{path} ({label})"));
}

/// The entry kind [`note_unlisted`] names: the listing's own type, or a
/// submodule the listing types `file`.
pub(super) fn unlisted_kind(entry: &ContentsEntry) -> &str {
    if entry.is_submodule() {
        "submodule"
    } else {
        &entry.kind
    }
}

pub(in crate::tools::gh_structure) fn entry_kind(entry: &ContentsEntry) -> Option<EntryKind> {
    if entry.is_submodule() {
        return None;
    }
    match entry.kind.as_str() {
        "file" => Some(EntryKind::File),
        "dir" => Some(EntryKind::Dir),
        _ => None,
    }
}
