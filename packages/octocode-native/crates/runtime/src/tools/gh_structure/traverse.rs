//! The tree walk: the recursive git tree, else a bounded Contents walk.
use super::*;

/// Deepest level a listing may reach (contract `maxDepth` maximum).
pub(super) fn max_listing_depth() -> usize {
    crate::contracts::query_schema_max(ToolId::GhStructure, None, "maxDepth")
}

/// One `include` glob: a case-insensitive glob over paths relative to the
/// listed `path`. Without a `/` it matches entry names at any depth; a bare
/// word (no glob metacharacters) matches names containing it.
pub(super) struct PathFilter {
    pub(super) matcher: globset::GlobMatcher,
    pub(super) by_name: bool,
}

impl PathFilter {
    pub(super) fn new(pattern: &str) -> Result<Self, ProviderError> {
        // The shared `include` coercion: a bare word matches names containing
        // it; anything else is a glob as written.
        let glob = crate::policy::include::include_glob(pattern.trim().trim_start_matches('/'));
        let pattern = glob.as_str();
        let by_name = !pattern.contains('/');
        let matcher = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .case_insensitive(true)
            .build()
            .map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::Validation,
                    format!("Invalid include glob \"{pattern}\": {error}"),
                )
            })?
            .compile_matcher();
        Ok(Self { matcher, by_name })
    }

    pub(super) fn matches(&self, path: &str) -> bool {
        if self.by_name {
            self.matcher
                .is_match(path.rsplit('/').next().unwrap_or(path))
        } else {
            self.matcher.is_match(path)
        }
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

/// The Contents API walk, one directory read per folder up to
/// [`MAX_DIRECTORY_FETCHES`], level by level with up to
/// [`READ_CONCURRENCY`] reads in flight; a complete walk is kept under its
/// commit ([`WalkMemo`]). After a truncated recursive tree (`split`), a folder with more
/// than one level left to list is read whole from its own recursive subtree,
/// so a large repository costs one read per subtree instead of per folder.
pub(super) async fn walk_contents(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    split: bool,
    context: &RequestContext,
) -> Result<Traversal, ProviderError> {
    use futures_util::StreamExt as _;
    let at = *listing;
    // Every tree page re-runs the walk; reuse a complete one of the commit.
    let memo = WalkMemo::of(provider, &at, context);
    if let Some(walk) = memo.get(provider).await {
        return Ok(walk);
    }
    let mut traversal = Traversal::default();
    // The directories of the next level: path and its tree SHA when its
    // parent listing named it. The first level is the requested root.
    let mut level = vec![(at.root.to_owned(), None::<String>)];
    let mut visited = HashSet::new();
    let mut fetches = 0usize;
    let mut depth = 1usize;
    while !level.is_empty() {
        level.retain(|(path, _)| visited.insert(path.clone()));
        let budget = MAX_DIRECTORY_FETCHES.saturating_sub(fetches);
        if level.len() > budget {
            traversal.fetch_limit = true;
            level.truncate(budget);
        }
        fetches += level.len();
        let required = depth == 1;
        let listings = futures_util::stream::iter(level.iter().map(|(path, _)| {
            provider.repository_contents(at.owner, at.repo, path, at.branch, context)
        }))
        .buffered(READ_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        let mut below = Vec::new();
        let mut subtrees = Vec::new();
        for ((path, sha), listing) in level.iter().zip(listings) {
            let mut listing = match listing {
                Ok(listing) => listing,
                Err(error) if !required && !access_failure(&error) => {
                    traversal.failed_subtrees += 1;
                    continue;
                }
                Err(error) => return Err(error),
            };
            // The Contents API answers a file path with a single object whose
            // path is the requested path itself; a tree cannot list a file.
            if required
                && !path.is_empty()
                && let [entry] = listing.entries.as_slice()
                && entry.path == *path
                && entry.kind != "dir"
            {
                return Err(not_a_directory(path, &entry.kind));
            }
            // The Contents API lists 1,000 entries of a directory at most; its
            // git tree lists them all.
            if listing.raw_entry_count >= CONTENTS_LIMIT {
                let whole = if fetches < MAX_DIRECTORY_FETCHES {
                    fetches += 1;
                    whole_directory(provider, &at, path, sha.as_deref(), context).await?
                } else {
                    None
                };
                match whole {
                    Some(entries) => listing.entries = entries,
                    None => traversal.contents_limit = true,
                }
            }
            for entry in listing.entries {
                let Some(kind) = entry_kind(&entry) else {
                    note_unlisted(&mut traversal, &entry.path, unlisted_kind(&entry));
                    continue;
                };
                traversal.entries.push(TreeEntry {
                    path: entry.path.clone(),
                    kind,
                    size: entry.size,
                });
                if kind != EntryKind::Dir || depth >= at.max_depth {
                    continue;
                }
                match entry.sha {
                    Some(sha) if split && at.max_depth - depth >= 2 => {
                        subtrees.push((entry.path, sha));
                    }
                    sha => below.push((entry.path, sha)),
                }
            }
        }
        // Folders with two or more levels left read whole from their own
        // recursive subtree, as many as the fetch budget allows; a subtree
        // that is truncated too (or unreadable) is walked like any folder.
        let budget = MAX_DIRECTORY_FETCHES.saturating_sub(fetches);
        let walked = subtrees.split_off(subtrees.len().min(budget));
        below.extend(walked.into_iter().map(|(path, sha)| (path, Some(sha))));
        fetches += subtrees.len();
        let trees = futures_util::stream::iter(
            subtrees
                .iter()
                .map(|(path, sha)| subtree(provider, &at, path, sha, context)),
        )
        .buffered(READ_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        for ((path, sha), tree) in subtrees.into_iter().zip(trees) {
            match tree? {
                Some(tree) => {
                    let read = traversal_from_git_tree(tree, &at);
                    traversal.entries.extend(read.entries);
                    traversal.omitted.extend(read.omitted);
                }
                None => below.push((path, Some(sha))),
            }
        }
        level = below;
        depth += 1;
    }
    if traversal.failed_subtrees == 0 {
        memo.put(provider, &traversal).await;
    }
    Ok(traversal)
}

/// Where a complete Contents walk of one listing is kept: by repository,
/// commit, root and depth, under the cache partition of the request. The
/// key's version names the walk's shape (omitted entries by path).
struct WalkMemo {
    partition: Option<octocode_github::Partition>,
    key: String,
    commit: String,
}

impl WalkMemo {
    fn of(provider: &GitHubProvider, listing: &Listing<'_>, context: &RequestContext) -> Self {
        let Listing {
            owner,
            repo,
            branch,
            root,
            max_depth,
        } = *listing;
        Self {
            partition: provider.transport.cache_partition(context, None).ok(),
            key: format!(
                "{}:{branch}:{root}:{max_depth}",
                repo_key("git-tree-walk-v2", owner, repo),
            ),
            commit: branch.to_owned(),
        }
    }

    async fn get(&self, provider: &GitHubProvider) -> Option<Traversal> {
        let cached = provider
            .transport
            .cache
            .get(self.partition.as_ref()?, &self.key)
            .await?;
        serde_json::from_slice(&cached.bytes).ok()
    }

    async fn put(&self, provider: &GitHubProvider, traversal: &Traversal) {
        if let (Some(partition), Ok(bytes)) = (&self.partition, serde_json::to_vec(traversal)) {
            provider
                .transport
                .cache
                .put(
                    partition,
                    self.key.clone(),
                    octocode_github::CachedContent {
                        etag: None,
                        bytes: bytes.into(),
                        resolved_ref: self.commit.clone(),
                        has_more: false,
                    },
                    octocode_github::Retention::Pinned,
                )
                .await;
        }
    }
}

/// A directory past the Contents API's entry cap, read whole from its own
/// (non-recursive) git tree as Contents entries. The root's tree is the
/// listed commit's; another directory's SHA is the one its parent listing
/// names, read from the parent when the walk started at it. `None` when the
/// tree is truncated or unreadable: the cap stays disclosed.
async fn whole_directory(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    dir: &str,
    sha: Option<&str>,
    context: &RequestContext,
) -> Result<Option<Vec<ContentsEntry>>, ProviderError> {
    let reference = match sha {
        Some(sha) => sha.to_owned(),
        None if dir.is_empty() => listing.branch.to_owned(),
        None => {
            let parent = dir.rsplit_once('/').map_or("", |(parent, _)| parent);
            let siblings = match provider
                .repository_contents(listing.owner, listing.repo, parent, listing.branch, context)
                .await
            {
                Ok(siblings) => siblings,
                Err(error) if access_failure(&error) => return Err(error),
                Err(_) => return Ok(None),
            };
            match siblings
                .entries
                .into_iter()
                .find(|entry| entry.path == dir && entry.kind == "dir")
                .and_then(|entry| entry.sha)
            {
                Some(sha) => sha,
                None => return Ok(None),
            }
        }
    };
    let tree = match provider
        .transport
        .get_tree(
            &TreeRequest {
                owner: listing.owner.into(),
                repo: listing.repo.into(),
                reference,
                recursive: false,
            },
            context,
        )
        .await
    {
        Ok(tree) if !tree.truncated => tree,
        Ok(_) => return Ok(None),
        Err(error) if access_failure(&error) => return Err(error),
        Err(_) => return Ok(None),
    };
    Ok(Some(
        tree.tree
            .into_iter()
            .map(|entry| ContentsEntry {
                path: if dir.is_empty() {
                    entry.path.clone()
                } else {
                    format!("{dir}/{}", entry.path)
                },
                kind: match git_kind(&entry) {
                    "blob" => "file".to_owned(),
                    "tree" => "dir".to_owned(),
                    "commit" => "submodule".to_owned(),
                    other => other.to_owned(),
                },
                name: entry.path,
                size: entry.size,
                sha: entry.sha,
                git_url: None,
            })
            .collect(),
    ))
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

/// A ref that did not resolve names the ref, like ghGetFileContent does.
pub(super) fn missing_ref(
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

pub(super) fn root_is_directory(
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
pub(super) fn not_a_directory(path: &str, kind: &str) -> ProviderError {
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

pub(super) fn traversal_from_git_tree(
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
fn git_kind(entry: &octocode_github::TreeEntry) -> &str {
    if entry.kind == "blob" && entry.mode.as_deref() == Some("120000") {
        "symlink"
    } else {
        &entry.kind
    }
}

/// A symlink or submodule has no listable content: name it by its
/// repository-relative `path` instead of dropping it silently.
fn note_unlisted(traversal: &mut Traversal, path: &str, kind: &str) {
    let label = match kind {
        "symlink" => "symlink",
        "submodule" | "commit" => "submodule",
        _ => return,
    };
    traversal.omitted.insert(format!("{path} ({label})"), 1);
}

/// The entry kind [`note_unlisted`] names: the listing's own type, or a
/// submodule the listing types `file`.
fn unlisted_kind(entry: &ContentsEntry) -> &str {
    if entry.is_submodule() {
        "submodule"
    } else {
        &entry.kind
    }
}

pub(super) fn entry_kind(entry: &ContentsEntry) -> Option<EntryKind> {
    if entry.is_submodule() {
        return None;
    }
    match entry.kind.as_str() {
        "file" => Some(EntryKind::File),
        "dir" => Some(EntryKind::Dir),
        _ => None,
    }
}
