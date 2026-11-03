//! The tree walk: the recursive git tree, else a bounded Contents walk.
use super::*;
mod contents;
mod entries;
pub(super) use contents::walk_contents;
pub(super) use entries::{entry_kind, missing_ref, not_a_directory, traversal_from_git_tree};
use entries::{git_kind, note_unlisted, unlisted_kind};

/// Deepest level a listing may reach: every level (the contract bounds no
/// `maxDepth`), spelled as the largest depth a lead can carry.
pub(super) fn max_listing_depth() -> usize {
    octocode_engine::num::saturating_usize(u32::MAX)
}

/// One `include` glob over repository-relative paths, matched as authored.
pub(super) struct PathFilter {
    pub(super) matcher: globset::GlobMatcher,
}

impl PathFilter {
    pub(super) fn new(pattern: &str) -> Result<Self, ProviderError> {
        let matcher = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::Validation,
                    format!("Invalid include glob \"{pattern}\": {error}"),
                )
            })?
            .compile_matcher();
        Ok(Self { matcher })
    }

    pub(super) fn matches(&self, path: &str) -> bool {
        self.matcher.is_match(path)
    }
}

/// The tree one listing walks: `root` of `owner/repo` at `branch`, `max_depth` levels deep.
#[derive(Clone, Copy)]
pub(super) struct Listing<'a> {
    pub(super) owner: &'a str,
    pub(super) repo: &'a str,
    pub(super) branch: &'a str,
    pub(super) root: &'a str,
    pub(super) max_depth: usize,
}

/// A failure no fallback read can recover: credentials, access, or rate.
pub(super) fn access_failure(error: &ProviderError) -> bool {
    matches!(
        error.kind,
        ProviderErrorKind::Authentication
            | ProviderErrorKind::Permission
            | ProviderErrorKind::RateLimited
    )
}

pub(super) async fn traverse(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    context: &RequestContext,
) -> Result<Traversal, ProviderError> {
    let mut split = false;
    if listing.max_depth > 1 {
        match recursive_tree(provider, listing, context).await? {
            DeepTree::Read(traversal) => return Ok(traversal),
            DeepTree::Truncated => split = true,
            DeepTree::Unavailable => {}
        }
    } else if let Some(level) = level_tree(provider, listing, context).await? {
        return Ok(traversal_from_git_tree(level, listing));
    }
    walk_contents(provider, listing, split, context).await
}

/// A one-level listing from the root's own git tree (`<commit>:<root>`,
/// not recursive): it types a symlink by its mode, as the deeper tree reads
/// do, where a Contents listing types it `file`, and it is not capped at
/// 1,000 entries. `None` (a missing path, a file, an unavailable tree)
/// leaves the listing to the Contents walk, which names what the root is.
async fn level_tree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    context: &RequestContext,
) -> Result<Option<octocode_github::TreeResponse>, ProviderError> {
    let reference = if listing.root.is_empty() {
        listing.branch.to_owned()
    } else {
        format!("{}:{}", listing.branch, listing.root)
    };
    let Some(mut tree) = read_tree(provider, listing, &reference, false, context).await? else {
        return Ok(None);
    };
    if !listing.root.is_empty() {
        for entry in &mut tree.tree {
            entry.path = format!("{}/{}", listing.root, entry.path);
        }
    }
    Ok(Some(tree))
}

/// What one recursive git tree read answered.
pub(super) enum DeepTree {
    Read(Traversal),
    /// Past GitHub's recursive-tree limit: smaller subtrees still read whole.
    Truncated,
    Unavailable,
}

/// A deep listing from one recursive git tree of the listed directory at a
/// commit (`<commit>:<root>`; the commit's own tree for the repository
/// root), so a listing below a large repository's root reads that directory's
/// tree, not a truncated tree of the whole repository and a read per
/// subtree. A truncated or unavailable tree (a path that is no directory
/// included) leaves the walk to the Contents API.
pub(super) async fn recursive_tree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    context: &RequestContext,
) -> Result<DeepTree, ProviderError> {
    let Listing { branch, root, .. } = *listing;
    let reference = if root.is_empty() {
        branch.to_owned()
    } else {
        format!("{branch}:{root}")
    };
    Ok(
        match fetch_tree(provider, listing, &reference, true, context).await? {
            TreeRead::Whole(mut tree) => {
                if !root.is_empty() {
                    for entry in &mut tree.tree {
                        entry.path = format!("{root}/{}", entry.path);
                    }
                }
                DeepTree::Read(traversal_from_git_tree(tree, listing))
            }
            TreeRead::Truncated => DeepTree::Truncated,
            TreeRead::Unavailable => DeepTree::Unavailable,
        },
    )
}

/// One subdirectory's whole subtree from a recursive git tree read by its
/// tree SHA, with
/// repo-relative paths; `None` when that subtree is truncated too or
/// unreadable, so the Contents walk descends into it instead.
async fn subtree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    dir: &str,
    sha: &str,
    context: &RequestContext,
) -> Result<Option<octocode_github::TreeResponse>, ProviderError> {
    let Some(mut tree) = read_tree(provider, listing, sha, true, context).await? else {
        return Ok(None);
    };
    for entry in &mut tree.tree {
        entry.path = format!("{dir}/{}", entry.path);
    }
    Ok(Some(tree))
}

/// One untruncated git tree of the listed repository at `reference` (a
/// commit or tree SHA, or `<commit>:<path>`); `None` when truncated or unreadable. Access
/// failures propagate.
async fn read_tree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    reference: &str,
    recursive: bool,
    context: &RequestContext,
) -> Result<Option<octocode_github::TreeResponse>, ProviderError> {
    Ok(
        match fetch_tree(provider, listing, reference, recursive, context).await? {
            TreeRead::Whole(tree) => Some(tree),
            TreeRead::Truncated | TreeRead::Unavailable => None,
        },
    )
}

/// What one git tree read answered.
enum TreeRead {
    Whole(octocode_github::TreeResponse),
    /// Past GitHub's recursive-tree limit.
    Truncated,
    /// Any other failure than access: the Contents walk reads instead.
    Unavailable,
}

/// Read the git tree, preserving truncation and access failures.
async fn fetch_tree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    reference: &str,
    recursive: bool,
    context: &RequestContext,
) -> Result<TreeRead, ProviderError> {
    match provider
        .transport
        .get_tree(
            &TreeRequest {
                owner: listing.owner.into(),
                repo: listing.repo.into(),
                reference: reference.into(),
                recursive,
            },
            context,
        )
        .await
    {
        Ok(tree) if !tree.truncated => Ok(TreeRead::Whole(tree)),
        Ok(_) => Ok(TreeRead::Truncated),
        Err(error) if access_failure(&error) => Err(error),
        Err(_) => Ok(TreeRead::Unavailable),
    }
}
