//! The folder-by-folder walk a listing takes when no whole recursive tree
//! covers it: each folder from its own git tree, or from its Contents listing
//! when no tree of it answers.
use super::*;

/// Folder and subtree reads one listing spends at most. A call has no wall
/// deadline, and every read draws on the caller's shared core quota: without
/// a bound, an `include` over a repository the size of Linux spent over 2,000
/// requests before answering. Past it the walk stops and says how many
/// folders it left unread ([`Traversal::unwalked`]): a disclosed cut, never a
/// silent one.
pub(in crate::gh_structure) const MAX_WALK_READS: usize = 300;

/// [`MAX_WALK_READS`], or a test's own on this thread.
fn walk_reads() -> usize {
    #[cfg(test)]
    if let Some(reads) = TEST_WALK_READS.with(std::cell::Cell::get) {
        return reads;
    }
    MAX_WALK_READS
}

#[cfg(test)]
thread_local! {
    /// Tests: a read budget smaller than any real tree needs.
    pub(in crate::gh_structure) static TEST_WALK_READS: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

/// The folder-by-folder walk, level by level with up to [`read_concurrency`]
/// folder reads in flight, within [`MAX_WALK_READS`]. After a truncated
/// recursive tree (`split`), a folder with more than one level left to list
/// is read whole from its own recursive subtree, so a large repository costs
/// one read per subtree instead of per folder.
pub(in crate::gh_structure) async fn walk_contents(
    provider: &GitHubProvider,
    listing: &Listing<'_>,
    split: bool,
    context: &RequestContext,
) -> Result<Traversal, ProviderError> {
    use futures_util::StreamExt as _;
    let at = *listing;
    let mut traversal = Traversal::default();
    // The folders of the next level. The first level is the requested root.
    let mut level = vec![at.root.to_owned()];
    let mut visited = HashSet::new();
    let mut depth = 1usize;
    let mut spent = 0usize;
    while !level.is_empty() {
        level.retain(|path| visited.insert(path.clone()));
        let room = walk_reads().saturating_sub(spent);
        if level.len() > room {
            traversal.unwalked += level.len() - room;
            #[cfg_attr(
                octocode_clippy_ratchet,
                expect(
                    clippy::disallowed_methods,
                    reason = "The folders past the read budget are counted in `unwalked`, which the listing discloses as partial"
                )
            )]
            {
                level.truncate(room);
            }
        }
        if level.is_empty() {
            break;
        }
        spent += level.len();
        let required = depth == 1;
        let reads = futures_util::stream::iter(
            level
                .iter()
                .map(|path| read_folder(provider, &at, path, required, context)),
        )
        .buffered(read_concurrency(provider))
        .collect::<Vec<_>>()
        .await;
        let mut below = Vec::new();
        let mut subtrees = Vec::new();
        for read in reads {
            let folder = match read {
                Ok(folder) => folder,
                Err(error) if !required && !terminal_failure(&error) => {
                    traversal.failed_subtrees += 1;
                    continue;
                }
                Err(error) => return Err(error),
            };
            absorb(&mut traversal, folder.listed);
            if depth >= at.max_depth {
                continue;
            }
            for (path, sha) in folder.folders {
                match sha {
                    Some(sha) if split && at.max_depth - depth >= 2 => subtrees.push((path, sha)),
                    _ => below.push(path),
                }
            }
        }
        // Folders with two or more levels left read whole from their own
        // recursive subtree; a subtree that is truncated too (or
        // unreadable) is walked like any folder.
        let room = walk_reads().saturating_sub(spent);
        if subtrees.len() > room {
            traversal.unwalked += subtrees.len() - room;
            #[cfg_attr(
                octocode_clippy_ratchet,
                expect(
                    clippy::disallowed_methods,
                    reason = "The subtrees past the read budget are counted in `unwalked`, which the listing discloses as partial"
                )
            )]
            {
                subtrees.truncate(room);
            }
        }
        spent += subtrees.len();
        let trees = futures_util::stream::iter(
            subtrees
                .iter()
                .map(|(path, sha)| subtree(provider, &at, path, sha, context)),
        )
        .buffered(read_concurrency(provider))
        .collect::<Vec<_>>()
        .await;
        for ((path, _), tree) in subtrees.into_iter().zip(trees) {
            match tree? {
                Some(tree) => absorb(&mut traversal, traversal_from_git_tree(tree, &at)),
                None => below.push(path),
            }
        }
        level = below;
        depth += 1;
    }
    Ok(traversal)
}

/// One folder of the walk: its entries, typed, and the folders below it with
/// the tree SHA its listing names for each.
struct Folder {
    listed: Traversal,
    folders: Vec<(String, Option<String>)>,
}

/// One folder's entries. Its own git tree (`<commit>:<path>`, not recursive)
/// types each entry by its mode and lists every one. Only when no tree of it
/// answers does its Contents listing stand in, which lists 1,000 entries at
/// most and types a link to a file `file`, where a contents read of the link
/// answers the target's bytes under the link's name ([`contents_folder`]).
/// A folder below the root is one a listing typed a directory; the root is
/// first typed from a tree above it ([`untreed_root`]).
async fn read_folder(
    provider: &GitHubProvider,
    at: &Listing<'_>,
    path: &str,
    required: bool,
    context: &RequestContext,
) -> Result<Folder, ProviderError> {
    if let Some(mut tree) =
        read_tree(provider, at, &folder_reference(at, path), false, context).await?
    {
        if !path.is_empty() {
            for entry in &mut tree.tree {
                entry.path = format!("{path}/{}", entry.path);
            }
        }
        let folders = tree
            .tree
            .iter()
            .filter(|entry| entry.kind == "tree")
            .map(|entry| (entry.path.clone(), entry.sha.clone()))
            .collect();
        return Ok(Folder {
            listed: traversal_from_git_tree(tree, at),
            folders,
        });
    }
    if required && !path.is_empty() {
        untreed_root(provider, at, path, context).await?;
    }
    let listing = provider
        .repository_contents(at.owner, at.repo, path, at.branch, context)
        .await?;
    // The Contents API answers a file path with a single object whose path
    // is the requested path itself; a tree cannot list a file.
    if required
        && !path.is_empty()
        && let [entry] = listing.entries.as_slice()
        && entry.path == path
        && entry.kind != "dir"
    {
        return Err(not_a_directory(path, &entry.kind));
    }
    Ok(contents_folder(listing))
}

/// The tree reference of folder `path` at the listed commit.
fn folder_reference(at: &Listing<'_>, path: &str) -> String {
    if path.is_empty() {
        at.branch.to_owned()
    } else {
        format!("{}:{path}", at.branch)
    }
}

/// What the requested root is when its own tree did not answer (GitHub
/// answers a tree of a file or link with a 422, of a missing path with a
/// 404), before any Contents read of it: a Contents read of a link to a file
/// answers the target's bytes. The deepest folder above it whose tree
/// answers names the next entry on its path. A file, link or submodule at
/// the root is refused as no directory, unread. The Contents read goes ahead
/// only where it answers no file's bytes: a directory's listing, or GitHub's
/// not-found for a path that is absent or lies below a file or link. With no
/// tree above it, or a folder on its path that answers no tree, nothing says
/// the root is no link, and it is not read; when the commit's own tree is
/// GitHub's not-found, that not-found is the answer (a commit or repository
/// GitHub does not have), as its Contents read would be.
async fn untreed_root(
    provider: &GitHubProvider,
    at: &Listing<'_>,
    path: &str,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let parent = |path: &str| path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
    let mut dir = parent(path);
    loop {
        let answer = provider
            .transport
            .get_tree(
                &TreeRequest {
                    owner: at.owner.into(),
                    repo: at.repo.into(),
                    reference: folder_reference(at, &dir),
                    recursive: false,
                },
                context,
            )
            .await;
        if let Ok(tree) = answer.as_ref()
            && !tree.truncated
        {
            let rest = if dir.is_empty() {
                path
            } else {
                path.strip_prefix(dir.as_str())
                    .and_then(|rest| rest.strip_prefix('/'))
                    .unwrap_or(path)
            };
            let (name, deeper) = rest
                .split_once('/')
                .map_or((rest, false), |(name, _)| (name, true));
            return match tree.tree.iter().find(|entry| entry.path == name) {
                Some(entry) if !deeper && entry.kind != "tree" => {
                    Err(not_a_directory(path, git_kind(entry)))
                }
                Some(entry) if deeper && entry.kind == "tree" => Err(unchecked_root(path)),
                _ => Ok(()),
            };
        }
        match answer {
            Err(error) if terminal_failure(&error) => return Err(error),
            Err(error) if dir.is_empty() && error.kind == ProviderErrorKind::NotFound => {
                return Err(error);
            }
            _ if dir.is_empty() => return Err(unchecked_root(path)),
            _ => dir = parent(&dir),
        }
    }
}

/// The refusal of a root no tree types: it is not read.
fn unchecked_root(path: &str) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unavailable,
        format!(
            "GitHub answered no git tree of \"{path}\" or of a folder on its path, so whether it is a symlink cannot be checked; it is not read."
        ),
    )
}

/// A folder's Contents listing as the walk's entries. A `file` entry may be
/// a link, which only a git tree tells apart, so with no tree it is left out
/// and counted ([`Traversal::unchecked`]); nothing of it is read. A
/// `symlink` entry (a link to anything but a file) is listed as a link.
fn contents_folder(listing: octocode_github::ContentsListing) -> Folder {
    let mut listed = Traversal {
        contents_limit: listing.raw_entry_count >= CONTENTS_LIMIT,
        ..Traversal::default()
    };
    let mut folders = Vec::new();
    for entry in listing.entries {
        let kind = match entry_kind(&entry) {
            Some(EntryKind::File) => {
                listed.unchecked += 1;
                continue;
            }
            Some(kind) => kind,
            None => {
                note_unlisted(&mut listed, &entry.path, unlisted_kind(&entry));
                continue;
            }
        };
        if kind == EntryKind::Dir {
            folders.push((entry.path.clone(), entry.sha.clone()));
        }
        listed.entries.push(TreeEntry {
            oid: file_oid(kind, entry.sha.as_deref()),
            path: entry.path,
            kind,
            size: entry.size,
        });
    }
    Folder { listed, folders }
}

/// One read's entries and coverage, added to the walk's.
fn absorb(traversal: &mut Traversal, read: Traversal) {
    traversal.entries.extend(read.entries);
    for (kind, paths) in read.omitted {
        traversal.omitted.entry(kind).or_default().extend(paths);
    }
    traversal.failed_subtrees += read.failed_subtrees;
    traversal.contents_limit |= read.contents_limit;
    traversal.unchecked += read.unchecked;
}
