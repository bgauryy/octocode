//! The Contents API walk: one directory read per folder, memoized per commit.
use super::*;

/// The Contents API walk, one directory read per folder the listing
/// reaches, level by level with up to [`READ_CONCURRENCY`] reads in flight;
/// a complete walk is kept under its commit ([`WalkMemo`]). No read count
/// ends it: GitHub sets no such rule, and its rate limits and the request
/// deadline are typed failures, never a silent cut. After a truncated recursive tree (`split`), a folder with more
/// than one level left to list is read whole from its own recursive subtree,
/// so a large repository costs one read per subtree instead of per folder.
pub(in crate::tools::gh_structure) async fn walk_contents(
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
    let mut depth = 1usize;
    while !level.is_empty() {
        level.retain(|(path, _)| visited.insert(path.clone()));
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
                match whole_directory(provider, &at, path, sha.as_deref(), context).await? {
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
        // recursive subtree; a subtree that is truncated too (or
        // unreadable) is walked like any folder.
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
/// key's version names the walk's shape (omitted entries as a path list;
/// v4: no walk is cut at a fetch count, so no stored walk is a cut one).
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
                repo_key("git-tree-walk-v4", owner, repo),
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
