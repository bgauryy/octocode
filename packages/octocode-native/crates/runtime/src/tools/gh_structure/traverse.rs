//! The tree walk: the recursive git tree, else a bounded Contents walk.
use super::*;
mod contents;
mod entries;
pub(super) use contents::walk_contents;
pub(super) use entries::{
    entry_kind, missing_ref, not_a_directory, root_is_directory, traversal_from_git_tree,
};
use entries::{git_kind, note_unlisted, unlisted_kind};

/// Deepest level a listing may reach (contract `maxDepth` maximum).
pub(super) fn max_listing_depth() -> usize {
    crate::contracts::query_schema_max(ToolId::GhStructure, None, "maxDepth")
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
    let resource = format!(
        "{}:{reference}",
        repo_key("git-tree-level", listing.owner, listing.repo)
    );
    let Some(mut tree) = read_tree(provider, listing, resource, &reference, false, context).await?
    else {
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

/// A deep listing from one recursive git tree (cached by commit); a
/// truncated or unavailable tree leaves the walk to the Contents API.
pub(super) async fn recursive_tree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    context: &RequestContext,
) -> Result<DeepTree, ProviderError> {
    let Listing {
        owner,
        repo,
        branch,
        root,
        ..
    } = *listing;
    let resource = format!("{}:{branch}", repo_key("git-tree", owner, repo));
    Ok(
        match fetch_tree(provider, listing, resource, branch, true, context).await? {
            TreeRead::Whole(tree) => {
                root_is_directory(&tree, root)?;
                DeepTree::Read(traversal_from_git_tree(tree, listing))
            }
            TreeRead::Truncated => DeepTree::Truncated,
            TreeRead::Unavailable => DeepTree::Unavailable,
        },
    )
}

/// One subdirectory's whole subtree from a recursive git tree read by its
/// tree SHA (content-addressed, so cached like a commit's tree), with
/// repo-relative paths; `None` when that subtree is truncated too or
/// unreadable, so the Contents walk descends into it instead.
async fn subtree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    dir: &str,
    sha: &str,
    context: &RequestContext,
) -> Result<Option<octocode_github::TreeResponse>, ProviderError> {
    let resource = format!(
        "{}:{sha}",
        repo_key("git-tree", listing.owner, listing.repo)
    );
    let Some(mut tree) = read_tree(provider, listing, resource, sha, true, context).await? else {
        return Ok(None);
    };
    for entry in &mut tree.tree {
        entry.path = format!("{dir}/{}", entry.path);
    }
    Ok(Some(tree))
}

/// One untruncated git tree of the listed repository at `reference` (a
/// commit or tree SHA, or `<commit>:<path>`: immutable either way), cached
/// under `resource`; `None` when it is truncated or unreadable. Access
/// failures propagate.
async fn read_tree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    resource: String,
    reference: &str,
    recursive: bool,
    context: &RequestContext,
) -> Result<Option<octocode_github::TreeResponse>, ProviderError> {
    Ok(
        match fetch_tree(provider, listing, resource, reference, recursive, context).await? {
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

/// The process's in-flight git tree reads, one per `(credential, resource)`;
/// a slot remembers that the first read came back truncated (never cached).
fn tree_flights()
-> &'static octocode_io::flight::KeyedFlight<(octocode_github::Partition, String), bool> {
    static FLIGHTS: octocode_io::flight::KeyedFlight<(octocode_github::Partition, String), bool> =
        octocode_io::flight::KeyedFlight::transient();
    &FLIGHTS
}

/// The git tree at `reference` (immutable: a commit or tree SHA, or
/// `<commit>:<path>`), from the cache under `resource` or one read that
/// stores an untruncated answer there. Single flight: concurrent rows on one
/// tree wait for the first read, then take what it cached (or its
/// truncation), so a tree of many MB downloads once. Access failures
/// propagate.
async fn fetch_tree(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    resource: String,
    reference: &str,
    recursive: bool,
    context: &RequestContext,
) -> Result<TreeRead, ProviderError> {
    let partition = provider.transport.cache_partition(context, None).ok();
    let ticket = partition
        .as_ref()
        .map(|partition| tree_flights().join((partition.clone(), resource.clone())));
    let mut turn = match &ticket {
        Some(ticket) => Some(context.budget.wait(ticket.turn()).await?),
        None => None,
    };
    if turn.as_deref() == Some(&true) {
        return Ok(TreeRead::Truncated);
    }
    if let Some(partition) = &partition
        && let Some(cached) = provider.transport.cache.get(partition, &resource).await
        && let Ok(tree) = serde_json::from_slice(&cached.bytes)
    {
        return Ok(TreeRead::Whole(tree));
    }
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
        Ok(tree) if !tree.truncated => {
            cache_tree(provider, context, resource, listing.branch, &tree).await;
            Ok(TreeRead::Whole(tree))
        }
        Ok(_) => {
            if let Some(truncated) = turn.as_deref_mut() {
                *truncated = true;
            }
            Ok(TreeRead::Truncated)
        }
        Err(error) if access_failure(&error) => Err(error),
        Err(_) => Ok(TreeRead::Unavailable),
    }
}

/// Keep a complete git tree under its commit (or tree SHA): it never
/// changes.
pub(super) async fn cache_tree(
    provider: &GitHubProvider,
    context: &RequestContext,
    resource: String,
    commit: &str,
    tree: &octocode_github::TreeResponse,
) {
    if let Ok(partition) = provider.transport.cache_partition(context, None)
        && let Ok(bytes) = serde_json::to_vec(tree)
    {
        provider
            .transport
            .cache
            .put(
                &partition,
                resource,
                octocode_github::CachedContent {
                    etag: None,
                    bytes: bytes.into(),
                    resolved_ref: commit.to_owned(),
                    has_more: false,
                },
                octocode_github::Retention::Pinned,
            )
            .await;
    }
}
