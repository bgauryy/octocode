//! Finding the symlink on a file's path before any of its bytes are asked
//! for. GitHub answers a contents read of a link to a file with the target's
//! bytes under the link's name (`type: "file"`, naming no target), and a read
//! through a symlinked directory with a 404. So a link is found by its mode in
//! a git tree, its target is read from its own blob (which holds only the
//! target path), and the target's bytes are asked for only by the target's
//! name, once the caller's rules allow that name.
use super::file::Links;
use super::payload::{resolve_link_target, symlink_chain_refused, symlink_refused};
use super::{ContentRequest, GitHubProvider};
use crate::{ProviderError, ProviderErrorKind, RequestContext, RequestSpec};
use reqwest::header::{ACCEPT, HeaderValue};
use serde::Deserialize;

/// Symlinks a read follows before it gives up (a loop, or a chain no one
/// means to read).
const MAX_LINK_HOPS: usize = 8;

/// The git file mode of a symlink.
const SYMLINK_MODE: &str = "120000";

/// GitHub's raw media type: a blob's own bytes instead of a JSON payload.
const RAW_MEDIA_TYPE: &str = "application/vnd.github.raw";

/// What a file read may ask for once the links on its path are known.
pub(super) enum Checked {
    /// Read `request`, the path a link chain ended at (`followed` names it
    /// when a link was followed). With `unlisted`, only GitHub's not-found
    /// may answer: a body would be bytes no check covered.
    Read {
        request: ContentRequest,
        followed: Option<String>,
        unlisted: Option<Unlisted>,
    },
    /// The chain ends at this path, which the caller withholds: nothing of
    /// it is asked for.
    Withheld(String),
}

/// Why a path is read only for GitHub's not-found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Unlisted {
    /// The commit's trees list no such path.
    Absent,
    /// No tree of any directory of the path answered (404 or 422 up to the
    /// root), so nothing says whether the path is a symlink.
    Unchecked,
}

/// A symlink found on a path: its own path and the blob holding its target.
struct Link {
    path: String,
    blob: String,
}

/// What the trees say of a whole path.
enum OnPath {
    /// The path, or one of its directories, is this symlink.
    Link(Link),
    /// Every directory of the path is a directory and the path is listed.
    Listed,
    /// The commit has no such path: the read answers GitHub's not-found.
    Absent,
    /// No tree of any directory of the path answered (404 or 422 up to the
    /// root): nothing says whether the path is a link.
    Unchecked,
}

/// A git tree, reading only what the link check needs of each entry.
#[derive(Deserialize)]
struct LinkTree {
    #[serde(default)]
    truncated: bool,
    tree: Vec<LinkEntry>,
}

#[derive(Deserialize)]
struct LinkEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    sha: Option<String>,
}

impl LinkTree {
    fn entry(&self, path: &str) -> Option<&LinkEntry> {
        self.tree.iter().find(|entry| entry.path == path)
    }
}

impl LinkEntry {
    fn is_link(&self) -> bool {
        self.mode.as_deref() == Some(SYMLINK_MODE)
    }

    fn is_directory(&self) -> bool {
        self.kind == "tree"
    }
}

/// The header that names a kept link proof's request apart from the read it
/// proves: never sent, so no origin answer can stand under its key.
const PROOF_HEADER: &str = "x-octocode-link-proof";

/// The request under which a file's link proof is kept: its own contents
/// read at the commit, named apart ([`PROOF_HEADER`]). A kept answer to it
/// says a GraphQL read proved the path a regular file at that commit, with
/// no symlink on it ([`super::batch`]).
pub(super) fn proof_spec(contents: &RequestSpec) -> RequestSpec {
    let mut proof = contents.clone();
    proof.headers.insert(
        reqwest::header::HeaderName::from_static(PROOF_HEADER),
        HeaderValue::from_static("regular-file"),
    );
    proof
}

/// A link proof's kept answer: the blob it proved.
pub(super) fn proof_page(blob: &str) -> crate::ResponsePage {
    crate::ResponsePage {
        status: 200,
        headers: reqwest::header::HeaderMap::new(),
        body: bytes::Bytes::copy_from_slice(blob.as_bytes()),
        next: None,
        redirected: false,
    }
}

/// `path`'s parent directory (`""` at the root) and its last segment.
pub(super) fn split_parent(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// `path` relative to directory `dir` (`""` is the root).
fn below<'a>(dir: &str, path: &'a str) -> Option<&'a str> {
    if dir.is_empty() {
        return Some(path);
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

/// The tree reference of directory `dir` at `commit`.
fn tree_reference(commit: &str, dir: &str) -> String {
    if dir.is_empty() {
        commit.to_owned()
    } else {
        format!("{commit}:{dir}")
    }
}

impl GitHubProvider {
    /// The symlink check of each distinct folder of `paths` at `commit`,
    /// asked once and concurrently, before a caller reads those files
    /// together: each read then finds its folder's commit-pinned tree among
    /// the saved answers instead of every read of one folder asking it at
    /// once. Nothing is read; a failure is left to the reads, which report it.
    pub async fn check_links_once_per_folder(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        paths: &[String],
        context: &RequestContext,
    ) {
        let mut folders = std::collections::HashSet::new();
        let request_of = |path: &String| ContentRequest {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            path: path.clone(),
            reference: Some(commit.to_owned()),
        };
        let request_of = &request_of;
        // A file a kept proof answers needs no check, and its folder none
        // for it ([`super::batch`]).
        let checks = paths
            .iter()
            .filter(|path| !self.proved_regular_file(&request_of(path), commit, context))
            .filter(|path| folders.insert(split_parent(path).0))
            .map(|path| async move {
                let _ = self.link_on_path(&request_of(path), commit, context).await;
            })
            .collect::<Vec<_>>();
        futures_util::future::join_all(checks).await;
    }

    /// What a read of `request` at `resolved_ref` may ask for. A refused
    /// link is an error naming its target. A path no tree could check is
    /// read only for GitHub's not-found ([`Checked::Read`] with `unlisted`).
    pub(super) async fn resolve_links(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
        context: &RequestContext,
        links: Links<'_>,
    ) -> Result<Checked, ProviderError> {
        let mut at = request.clone();
        let mut followed: Option<String> = None;
        // A kept proof says the path is a regular file at this commit with
        // no link on it: nothing to follow, refuse or withhold.
        if matches!(links, Links::Listed)
            || self.proved_regular_file(request, resolved_ref, context)
        {
            return Ok(Checked::Read {
                request: at,
                followed,
                unlisted: None,
            });
        }
        let mut last_target = None;
        for _ in 0..=MAX_LINK_HOPS {
            let link = match self.link_on_path(&at, resolved_ref, context).await? {
                OnPath::Link(link) => link,
                found => {
                    if let (Links::Follow(withholds), Some(target)) = (links, &followed)
                        && withholds(target)
                    {
                        return Ok(Checked::Withheld(target.clone()));
                    }
                    return Ok(Checked::Read {
                        request: at,
                        followed,
                        unlisted: match found {
                            OnPath::Absent => Some(Unlisted::Absent),
                            OnPath::Unchecked => Some(Unlisted::Unchecked),
                            OnPath::Listed | OnPath::Link(_) => None,
                        },
                    });
                }
            };
            let text = self.link_text(&at, &link, context).await?;
            let target = resolve_link_target(&link.path, &text);
            let (Links::Follow(_), Some(target)) = (links, target.as_deref()) else {
                return Err(symlink_refused(&link.path, &text, target.as_deref()));
            };
            let rest = at.path.strip_prefix(link.path.as_str()).unwrap_or_default();
            at.path = format!("{target}{rest}");
            last_target = Some(text);
            followed = Some(at.path.clone());
        }
        Err(symlink_chain_refused(
            &request.path,
            MAX_LINK_HOPS,
            last_target.as_deref(),
        ))
    }

    /// Whether a kept link proof ([`proof_spec`]) answers `request.path` at
    /// `resolved_ref` with nothing sent.
    fn proved_regular_file(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
        context: &RequestContext,
    ) -> bool {
        self.contents_spec(request, resolved_ref)
            .is_ok_and(|spec| self.transport.answers_unasked(&proof_spec(&spec), context))
    }

    /// The symlink on `request.path`, if any. A whole recursive tree this
    /// credential already holds for the commit answers without a request.
    /// Otherwise the leaf's parent directory's git tree answers: GitHub
    /// serves no tree through a symlinked directory, so a parent tree that
    /// answers is a real directory with no link above it, and every file of
    /// it shares that one commit-pinned read. Only when it does not answer
    /// (404 or 422) are the directories above asked, deepest first, for the
    /// directory link that hides it.
    async fn link_on_path(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
        context: &RequestContext,
    ) -> Result<OnPath, ProviderError> {
        if let Some((dir, tree)) = self.held_tree(request, resolved_ref, context).await {
            return Ok(walk_tree(&dir, &tree, &request.path));
        }
        let (mut dir, _) = split_parent(&request.path);
        loop {
            if let Some(tree) = self
                .directory_tree(request, dir, resolved_ref, context)
                .await?
            {
                return walk_level(dir, &tree, &request.path, resolved_ref);
            }
            if dir.is_empty() {
                return Ok(OnPath::Unchecked);
            }
            dir = split_parent(dir).0;
        }
    }

    /// The deepest whole recursive tree of the commit held for a directory
    /// above the path, with that directory. Nothing is asked.
    async fn held_tree(
        &self,
        request: &ContentRequest,
        resolved_ref: &str,
        context: &RequestContext,
    ) -> Option<(String, LinkTree)> {
        let (mut dir, _) = split_parent(&request.path);
        loop {
            let reference = tree_reference(resolved_ref, dir);
            if let Ok(spec) = self.tree_spec(request, &reference, true)
                && self.transport.answers_unasked(&spec, context)
                && let Ok(page) = self.transport.execute(spec, context).await
                && let Ok(tree) = serde_json::from_slice::<LinkTree>(&page.body)
                && !tree.truncated
            {
                return Some((dir.to_owned(), tree));
            }
            if dir.is_empty() {
                return None;
            }
            dir = split_parent(dir).0;
        }
    }

    fn tree_spec(
        &self,
        request: &ContentRequest,
        reference: &str,
        recursive: bool,
    ) -> Result<RequestSpec, ProviderError> {
        let mut url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "trees",
            reference,
        ])?;
        if recursive {
            url.query_pairs_mut().append_pair("recursive", "1");
        }
        Ok(RequestSpec::get(url))
    }

    /// The git tree of directory `dir` at the commit (not recursive): every
    /// entry with its mode, whatever the directory's size. `None` when
    /// GitHub answers that `dir` is no directory at this commit (404, or 422
    /// for a file or link), which checks nothing; a truncated tree checks
    /// nothing either and fails the read.
    async fn directory_tree(
        &self,
        request: &ContentRequest,
        dir: &str,
        resolved_ref: &str,
        context: &RequestContext,
    ) -> Result<Option<LinkTree>, ProviderError> {
        let spec = self.tree_spec(request, &tree_reference(resolved_ref, dir), false)?;
        let page = match self.transport.execute(spec, context).await {
            Ok(page) => page,
            Err(error) if matches!(error.status, Some(404 | 422)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let tree: LinkTree = serde_json::from_slice(&page.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub tree response")
        })?;
        if tree.truncated {
            return Err(ProviderError::new(
                ProviderErrorKind::Unavailable,
                format!(
                    "Directory \"{}\" is too large for GitHub to list whole; whether \"{}\" is a symlink cannot be checked, so it is not read.",
                    if dir.is_empty() { "/" } else { dir },
                    request.path
                ),
            ));
        }
        Ok(Some(tree))
    }

    /// The target path a symlink's own blob holds. The blob is the link's
    /// (content-addressed, so every read of it shares one answer): its bytes
    /// are a path, never the target's text.
    async fn link_text(
        &self,
        request: &ContentRequest,
        link: &Link,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        let url = self.transport.endpoint().rest(&[
            "repos",
            &request.owner,
            &request.repo,
            "git",
            "blobs",
            &link.blob,
        ])?;
        let mut spec = RequestSpec::get(url);
        spec.headers
            .insert(ACCEPT, HeaderValue::from_static(RAW_MEDIA_TYPE));
        let page = self.transport.execute(spec, context).await?;
        let bytes = super::file::blob_bytes(&page)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                format!(
                    "Path \"{}\" is a symlink whose target is not a UTF-8 path.",
                    link.path
                ),
            )
        })
    }
}

/// What directory `dir`'s own (non-recursive) tree says of `path` below it.
/// Every directory above `dir` is a real one (its tree answered); when `dir`
/// is not the path's parent, the directory between them answered no tree, so
/// it must be a link or absent here.
fn walk_level(
    dir: &str,
    tree: &LinkTree,
    path: &str,
    resolved_ref: &str,
) -> Result<OnPath, ProviderError> {
    let Some(rest) = below(dir, path) else {
        return Ok(OnPath::Absent);
    };
    let (name, deeper) = rest
        .split_once('/')
        .map_or((rest, false), |(name, _)| (name, true));
    let child = if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    };
    Ok(match tree.entry(name) {
        Some(entry) if entry.is_link() => link_at(child, entry)?,
        Some(entry) if deeper && entry.is_directory() => {
            return Err(ProviderError::new(
                ProviderErrorKind::Unavailable,
                format!(
                    "GitHub lists \"{child}\" as a directory at {resolved_ref} but answers no tree for it; whether \"{path}\" is a symlink cannot be checked, so it is not read."
                ),
            ));
        }
        Some(_) if !deeper => OnPath::Listed,
        // A file or submodule where a directory is named, or no entry.
        _ => OnPath::Absent,
    })
}

/// What a whole recursive tree held for directory `dir` says of `path`
/// below it: each directory of the path from `dir` down, as a recursive tree
/// lists a link without descending into it.
fn walk_tree(dir: &str, tree: &LinkTree, path: &str) -> OnPath {
    let Some(rest) = below(dir, path) else {
        return OnPath::Absent;
    };
    let mut relative = String::new();
    let mut segments = rest.split('/').peekable();
    while let Some(segment) = segments.next() {
        if !relative.is_empty() {
            relative.push('/');
        }
        relative.push_str(segment);
        let last = segments.peek().is_none();
        match tree.entry(&relative) {
            Some(entry) if entry.is_link() => {
                let child = if dir.is_empty() {
                    relative.clone()
                } else {
                    format!("{dir}/{relative}")
                };
                return link_at(child, entry).unwrap_or(OnPath::Unchecked);
            }
            Some(_) if last => return OnPath::Listed,
            Some(entry) if entry.is_directory() => {}
            _ => return OnPath::Absent,
        }
    }
    OnPath::Absent
}

fn link_at(path: String, entry: &LinkEntry) -> Result<OnPath, ProviderError> {
    let blob = entry.sha.clone().ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::Unavailable,
            format!(
                "GitHub lists \"{path}\" as a symlink with no blob; its target cannot be read."
            ),
        )
    })?;
    Ok(OnPath::Link(Link { path, blob }))
}
