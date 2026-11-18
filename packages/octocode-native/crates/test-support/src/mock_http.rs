//! wiremock mounts and responders shared by every crate's HTTP tests: the
//! github crate's own tests, and octocode-native's unit and integration tests
//! through the `test-support` feature (enabled only by dev builds).
//!
//! Every responder logs each request's arrival, so a test can count hits or
//! check request spacing without a responder of its own.
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Answer `GET route` on `server` with `status` and a JSON `body`.
///
/// A file (or submodule) answer of a contents route
/// (`.../repos/<owner>/<repo>/contents/<path>`) also lists that path in the
/// repository's git trees ([`list_in_trees`]), as GitHub's tree of the commit
/// does: every file read asks its folder's tree first, to find a symlink on
/// its path.
pub async fn mount_json(
    server: &MockServer,
    route: impl Into<String>,
    status: u16,
    body: impl serde::Serialize,
) {
    let route = route.into();
    let body = serde_json::to_value(body).unwrap_or_default();
    if status == 200
        && let Some((repository, at)) = contents_route(&route)
        && !at.is_empty()
    {
        // A file, or a submodule: an entry of its folder's tree either way,
        // typed as the tree types it (a submodule is the commit it pins). A
        // directory listing lists nothing: a test answering one may mean
        // the walk a listing takes when no tree answers (`mount_folder_tree`
        // gives such a folder its own tree).
        let node = match body["type"].as_str() {
            Some("file") => Some(GitNode::File(Vec::new())),
            Some("submodule") => Some(GitNode::Submodule(
                body["sha"]
                    .as_str()
                    .map_or_else(|| GitRepository::blob_sha(&at), str::to_owned),
            )),
            _ => None,
        };
        if let Some(node) = node {
            list_in_repository_trees(server, &repository, &at, node).await;
        }
    }
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

/// The repository route (`repos/<owner>/<repo>` or `repositories/<id>`) and
/// the decoded path of a contents route.
fn contents_route(route: &str) -> Option<(String, String)> {
    let (repository, tail) = if let Some((_, rest)) = route.split_once("/repos/") {
        let mut parts = rest.splitn(3, '/');
        let (owner, repo, tail) = (parts.next()?, parts.next()?, parts.next()?);
        (format!("repos/{owner}/{repo}"), tail)
    } else {
        let (_, rest) = route.split_once("/repositories/")?;
        let (id, tail) = rest.split_once('/')?;
        (format!("repositories/{id}"), tail)
    };
    let at = tail.strip_prefix("contents")?;
    let at = at.strip_prefix('/').unwrap_or(at);
    Some((repository, percent_decoded(at)))
}

/// The entries each server answers contents reads for, by server and
/// repository route. A server's entry lives as long as the tree responder it
/// mounted: a later server on the same address starts empty.
type Listed = Arc<Mutex<GitRepository>>;
type ListedByServer = std::collections::HashMap<String, std::sync::Weak<Mutex<GitRepository>>>;
static LISTED: std::sync::LazyLock<Mutex<ListedByServer>> =
    std::sync::LazyLock::new(Mutex::default);

/// List `file` in the git trees `server` answers for `owner/repo` (at any
/// commit), as GitHub's tree lists every file of the commit: a regular file
/// (mode `100644`) and every directory above it. A test's own tree routes
/// answer first.
pub async fn list_in_trees(server: &MockServer, owner: &str, repo: &str, file: &str) {
    list_in_repository_trees(
        server,
        &format!("repos/{owner}/{repo}"),
        file,
        GitNode::File(Vec::new()),
    )
    .await;
}

async fn list_in_repository_trees(
    server: &MockServer,
    repository: &str,
    entry: &str,
    node: GitNode,
) {
    let key = format!("{}/{repository}/", server.uri());
    let (listed, new) = {
        let mut by_server = LISTED.lock().unwrap_or_else(PoisonError::into_inner);
        match by_server.get(&key).and_then(std::sync::Weak::upgrade) {
            Some(listed) => (listed, false),
            None => {
                let listed: Listed = Arc::default();
                by_server.insert(key, Arc::downgrade(&listed));
                (listed, true)
            }
        }
    };
    listed
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .nodes
        .insert(entry.to_owned(), node);
    if new {
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex(format!(
                "/{repository}/git/trees/"
            )))
            .respond_with(ListedTrees(listed))
            .with_priority(10)
            .mount(server)
            .await;
    }
}

/// Answer the git tree of folder `dir` (`""` is the root) of the repository
/// at API route `repository` (such as `/api/v3/repos/a/b`) at `commit`, read
/// one level (no `recursive`), with the entries a test's Contents listing of
/// that folder gives (`entries`: GitHub's directory listing shape), each
/// typed by its mode as GitHub's tree types it: `file` a blob (`100644`),
/// `dir` a tree, `symlink` a blob of mode `120000`, a submodule a commit.
/// The recursive tree stays the test's own, so a listing it leaves
/// unanswered walks folder by folder and types each folder from this tree.
pub async fn mount_folder_tree(
    server: &MockServer,
    repository: &str,
    commit: &str,
    dir: &str,
    entries: &serde_json::Value,
) {
    let tree = entries
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            let path = entry["path"].as_str().unwrap_or_default();
            let name = entry["name"]
                .as_str()
                .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path));
            let sha = entry["sha"]
                .as_str()
                .map_or_else(|| GitRepository::blob_sha(path), str::to_owned);
            let submodule = entry["type"] == "submodule"
                || entry["git_url"]
                    .as_str()
                    .is_some_and(|url| url.contains("/git/trees/"));
            let (kind, mode) = match entry["type"].as_str() {
                _ if submodule => ("commit", "160000"),
                Some("dir") => ("tree", "040000"),
                Some("symlink") => ("blob", "120000"),
                _ => ("blob", "100644"),
            };
            let mut row = serde_json::json!({"path": name, "mode": mode, "type": kind, "sha": sha});
            if kind == "blob"
                && let Some(size) = entry.get("size").filter(|size| size.is_u64())
            {
                row["size"] = size.clone();
            }
            row
        })
        .collect::<Vec<_>>();
    let reference = if dir.is_empty() {
        commit.to_owned()
    } else {
        format!("{commit}:{dir}")
    };
    let route = format!("{repository}/git/trees/{reference}");
    Mock::given(method("GET"))
        .and(move |request: &Request| percent_decoded(request.url.path()) == route)
        .and(wiremock::matchers::query_param_is_missing("recursive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": commit, "truncated": false, "tree": tree
        })))
        .mount(server)
        .await;
}

/// The responder [`list_in_trees`] mounts.
struct ListedTrees(Listed);

impl Respond for ListedTrees {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let path = percent_decoded(request.url.path());
        let Some((_, reference)) = path.split_once("/git/trees/") else {
            return not_found();
        };
        let (commit, dir) = reference.split_once(':').unwrap_or((reference, ""));
        let repository = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let recursive = request.url.query_pairs().any(|(key, _)| key == "recursive");
        ServedRepository {
            repository,
            prefix: String::new(),
            commit: commit.to_owned(),
        }
        .tree(&format!("{commit}:{dir}"), recursive)
    }
}

/// Answer `GET route` on `server` as a GitHub search over `items` does:
/// each request's `page`/`per_page` slice (GitHub's default `per_page` is
/// 30), with the full `total_count`.
pub async fn mount_search_index(
    server: &MockServer,
    route: impl Into<String>,
    items: Vec<serde_json::Value>,
) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(SearchIndex(items))
        .mount(server)
        .await;
}

/// The responder of [`mount_search_index`].
pub struct SearchIndex(pub Vec<serde_json::Value>);

impl Respond for SearchIndex {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let number = |name: &str, default: usize| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
                .unwrap_or(default)
        };
        let (page, per_page) = (number("page", 1).max(1), number("per_page", 30));
        let items = self
            .0
            .iter()
            .skip((page - 1) * per_page)
            .take(per_page)
            .cloned()
            .collect::<Vec<_>>();
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": self.0.len(), "incomplete_results": false, "items": items
        }))
    }
}

/// When each request reached a responder, in arrival order. Clones share one log.
#[derive(Clone, Default)]
pub struct Arrivals(Arc<Mutex<Vec<Instant>>>);

impl Arrivals {
    fn record(&self) -> usize {
        let mut log = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        log.push(Instant::now());
        log.len() - 1
    }

    /// The arrival instants so far.
    pub fn times(&self) -> Vec<Instant> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Requests answered so far.
    pub fn count(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).len()
    }
}

/// Answers the n-th request with the n-th template; the last template answers
/// every request after it (`Sequence::new(vec![failure, success])` fails once).
#[derive(Clone)]
pub struct Sequence {
    pub templates: Vec<ResponseTemplate>,
    pub arrivals: Arrivals,
}

impl Sequence {
    pub fn new(templates: Vec<ResponseTemplate>) -> Self {
        assert!(!templates.is_empty(), "a sequence needs a template");
        Self {
            templates,
            arrivals: Arrivals::default(),
        }
    }
}

impl Respond for Sequence {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let index = self.arrivals.record().min(self.templates.len() - 1);
        self.templates[index].clone()
    }
}

/// Answers every request with `template` once `after` has passed. With
/// `arrived`, it also wakes that notify as each request arrives, so a test can
/// act while the answer is still in flight instead of after a guessed sleep.
#[derive(Clone)]
pub struct Delayed {
    pub after: Duration,
    pub template: ResponseTemplate,
    pub arrivals: Arrivals,
    pub arrived: Option<Arc<tokio::sync::Notify>>,
}

impl Delayed {
    pub fn new(after: Duration, template: ResponseTemplate) -> Self {
        Self {
            after,
            template,
            arrivals: Arrivals::default(),
            arrived: None,
        }
    }
}

impl Respond for Delayed {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.arrivals.record();
        if let Some(arrived) = &self.arrived {
            arrived.notify_one();
        }
        self.template.clone().set_delay(self.after)
    }
}

/// One repository at one commit, answered as GitHub's REST API answers it
/// (shapes recorded live on public repositories):
/// - `git/trees/<commit>[:<dir>]` lists a directory's entries with their
///   modes: a symlink is a `blob` of mode `120000` whose `sha` is the link's
///   own blob, a submodule a `commit` of mode `160000` (no size) whose `sha`
///   is the commit it pins. A tree through a symlinked directory, or of a
///   submodule, is a 404; a tree of a file or link is a 422. `?recursive=1`
///   lists every entry below it.
/// - `git/blobs/<sha>` answers the blob's own bytes (`text/plain`): a link's
///   blob holds its target path.
/// - `contents/<path>` reads a file. A link to a file answers the TARGET's
///   bytes under the link's name (`type: "file"`, the link's `sha`, no
///   `target`); a link to anything else answers `type: "symlink"` with its
///   `target` (the raw read answers the link text); a path through a
///   symlinked directory is a 404. A directory answers its listing, which
///   types a link to a file as `file`.
#[derive(Clone, Debug, Default)]
pub struct GitRepository {
    nodes: std::collections::BTreeMap<String, GitNode>,
}

#[derive(Clone, Debug)]
enum GitNode {
    File(Vec<u8>),
    Link(String),
    /// A submodule and the commit it pins.
    Submodule(String),
}

/// What a repository path is at the commit.
enum GitPath<'a> {
    File(&'a [u8]),
    /// A symlink and the text it holds.
    Link(&'a str),
    /// A submodule and the commit it pins.
    Submodule(&'a str),
    Directory,
    /// A directory above it is a file or a symlink.
    Through,
    Missing,
}

impl GitRepository {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A regular file at `path` holding `text`.
    #[must_use]
    pub fn file(self, path: &str, text: &str) -> Self {
        self.file_bytes(path, text.as_bytes())
    }

    /// A regular file at `path` holding `bytes`.
    #[must_use]
    pub fn file_bytes(mut self, path: &str, bytes: &[u8]) -> Self {
        self.nodes
            .insert(path.to_owned(), GitNode::File(bytes.to_vec()));
        self
    }

    /// A symlink at `path` holding `target` (relative to its directory, as
    /// git stores it).
    #[must_use]
    pub fn link(mut self, path: &str, target: &str) -> Self {
        self.nodes
            .insert(path.to_owned(), GitNode::Link(target.to_owned()));
        self
    }

    /// The blob SHA the trees list for the entry at `path`.
    #[must_use]
    pub fn blob_sha(path: &str) -> String {
        use sha2::Digest as _;
        // The first 20 bytes: a git blob SHA's 40 hex digits.
        hex::encode(&sha2::Sha256::digest(format!("blob {path}"))[..20])
    }

    /// Answer this repository's git trees, blobs and contents reads at
    /// `commit` under `repos/<owner>/<repo>` on `server`.
    pub async fn mount(self, server: &MockServer, owner: &str, repo: &str, commit: &str) {
        self.mount_routes(server, owner, repo, commit, true).await;
    }

    /// Answer only this repository's git trees and its links' blobs: for a
    /// test that answers its contents and file blobs itself. A file read
    /// asks its folder's tree first, to find a symlink on its path.
    pub async fn mount_trees(self, server: &MockServer, owner: &str, repo: &str, commit: &str) {
        self.mount_routes(server, owner, repo, commit, false).await;
    }

    async fn mount_routes(
        self,
        server: &MockServer,
        owner: &str,
        repo: &str,
        commit: &str,
        contents: bool,
    ) {
        let prefix = format!("/repos/{owner}/{repo}/");
        // Blobs answer only this repository's own SHAs (all of them, or with
        // `contents` off only its links'), so a test's own blob routes stay.
        let blobs = self
            .nodes
            .iter()
            .filter(|(_, node)| contents || matches!(node, GitNode::Link(_)))
            .map(|(path, _)| Self::blob_sha(path))
            .collect::<Vec<_>>();
        let mut routes = vec![format!("{prefix}git/trees/")];
        if !blobs.is_empty() {
            routes.push(format!("{prefix}git/blobs/({})$", blobs.join("|")));
        }
        if contents {
            routes.push(format!("{prefix}contents(/|$)"));
        }
        for route in routes {
            Mock::given(method("GET"))
                .and(wiremock::matchers::path_regex(route))
                .respond_with(ServedRepository {
                    repository: self.clone(),
                    prefix: prefix.clone(),
                    commit: commit.to_owned(),
                })
                // Ahead of any catch-all route a test mounts for other paths
                // (default priority 5); a test's own priority-1 route still
                // answers first.
                .with_priority(2)
                .mount(server)
                .await;
        }
    }

    fn path(&self, path: &str) -> GitPath<'_> {
        let mut prefix = String::new();
        let segments = path.split('/').collect::<Vec<_>>();
        for (at, segment) in segments.iter().enumerate() {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(segment);
            let last = at + 1 == segments.len();
            match self.nodes.get(&prefix) {
                Some(GitNode::File(bytes)) if last => return GitPath::File(bytes),
                Some(GitNode::Link(text)) if last => return GitPath::Link(text),
                Some(GitNode::Submodule(commit)) if last => return GitPath::Submodule(commit),
                Some(_) => return GitPath::Through,
                None => {}
            }
        }
        if path.is_empty()
            || self
                .nodes
                .keys()
                .any(|key| key.starts_with(&format!("{path}/")))
        {
            GitPath::Directory
        } else {
            GitPath::Missing
        }
    }

    /// The entries below directory `dir`: its children, or with `recursive`
    /// every entry under it, as `(path relative to dir, node or None for a
    /// directory)`.
    fn entries(&self, dir: &str, recursive: bool) -> Vec<(String, Option<&GitNode>)> {
        let mut out: Vec<(String, Option<&GitNode>)> = Vec::new();
        for (path, node) in &self.nodes {
            let relative = if dir.is_empty() {
                path.as_str()
            } else {
                match path
                    .strip_prefix(dir)
                    .and_then(|rest| rest.strip_prefix('/'))
                {
                    Some(relative) => relative,
                    None => continue,
                }
            };
            let segments = relative.split('/').collect::<Vec<_>>();
            let depth = if recursive { segments.len() } else { 1 };
            for level in 1..=depth {
                let name = segments[..level].join("/");
                let entry = (level == segments.len()).then_some(node);
                if !out.iter().any(|(listed, _)| *listed == name) {
                    out.push((name, entry));
                }
            }
        }
        out
    }

    /// The repository path link `link` holding `text` reaches, `None`
    /// outside the repository.
    fn resolve(link: &str, text: &str) -> Option<String> {
        if text.starts_with('/') {
            return None;
        }
        let mut parts = link.split('/').collect::<Vec<_>>();
        parts.pop();
        for part in text.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                name => parts.push(name),
            }
        }
        (!parts.is_empty()).then(|| parts.join("/"))
    }
}

/// The responder [`GitRepository::mount`] mounts.
struct ServedRepository {
    repository: GitRepository,
    prefix: String,
    commit: String,
}

fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(value) = text
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(value);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(404).set_body_json(serde_json::json!({
        "message": "Not Found", "status": "404"
    }))
}

fn base64_text(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

impl ServedRepository {
    fn tree(&self, reference: &str, recursive: bool) -> ResponseTemplate {
        let (commit, dir) = reference.split_once(':').unwrap_or((reference, ""));
        if commit != self.commit {
            return not_found();
        }
        match self.repository.path(dir) {
            GitPath::Directory => {}
            GitPath::File(_) | GitPath::Link(_) => {
                return ResponseTemplate::new(422).set_body_json(serde_json::json!({
                    "message": "Invalid object requested. SHA must identify a commit or a tree.",
                    "status": "422"
                }));
            }
            GitPath::Submodule(_) | GitPath::Through | GitPath::Missing => return not_found(),
        }
        let tree = self
            .repository
            .entries(dir, recursive)
            .into_iter()
            .map(|(name, node)| {
                let full = if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{dir}/{name}")
                };
                let sha = GitRepository::blob_sha(&full);
                match node {
                    None => serde_json::json!({"path": name, "mode": "040000", "type": "tree", "sha": sha}),
                    Some(GitNode::File(bytes)) => serde_json::json!({
                        "path": name, "mode": "100644", "type": "blob", "sha": sha, "size": bytes.len()
                    }),
                    Some(GitNode::Link(text)) => serde_json::json!({
                        "path": name, "mode": "120000", "type": "blob", "sha": sha, "size": text.len()
                    }),
                    Some(GitNode::Submodule(commit)) => serde_json::json!({
                        "path": name, "mode": "160000", "type": "commit", "sha": commit
                    }),
                }
            })
            .collect::<Vec<_>>();
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": self.commit, "truncated": false, "tree": tree
        }))
    }

    fn blob(&self, sha: &str) -> ResponseTemplate {
        let found = self
            .repository
            .nodes
            .iter()
            .find(|(path, _)| GitRepository::blob_sha(path) == sha);
        let bytes = match found {
            Some((_, GitNode::File(bytes))) => bytes.clone(),
            Some((_, GitNode::Link(text))) => text.as_bytes().to_vec(),
            Some((_, GitNode::Submodule(_))) | None => return not_found(),
        };
        ResponseTemplate::new(200).set_body_raw(bytes, "text/plain; charset=utf-8")
    }

    fn contents(&self, path: &str, raw: bool) -> ResponseTemplate {
        let repository = &self.repository;
        let file = |name: &str, sha: String, bytes: &[u8]| {
            if raw {
                ResponseTemplate::new(200)
                    .set_body_raw(bytes.to_vec(), "application/vnd.github.raw; charset=utf-8")
                    .insert_header("etag", format!("\"{sha}\""))
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "type": "file", "name": name.rsplit('/').next(), "path": name, "sha": sha,
                    "size": bytes.len(), "encoding": "base64", "content": base64_text(bytes)
                }))
            }
        };
        match repository.path(path) {
            GitPath::File(bytes) => file(path, GitRepository::blob_sha(path), bytes),
            GitPath::Link(text) => {
                let sha = GitRepository::blob_sha(path);
                match GitRepository::resolve(path, text)
                    .as_deref()
                    .map(|target| repository.path(target))
                {
                    Some(GitPath::File(target_bytes)) => file(path, sha, target_bytes),
                    _ if raw => ResponseTemplate::new(200)
                        .set_body_raw(
                            text.as_bytes().to_vec(),
                            "application/vnd.github.raw; charset=utf-8",
                        )
                        .insert_header("etag", format!("\"{sha}\"")),
                    _ => ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "type": "symlink", "name": path.rsplit('/').next(), "path": path,
                        "sha": sha, "size": text.len(), "target": text,
                        "content": null, "encoding": null
                    })),
                }
            }
            GitPath::Submodule(commit) => {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "type": "submodule", "name": path.rsplit('/').next(), "path": path,
                    "sha": commit, "size": 0
                }))
            }
            GitPath::Directory => {
                let listing = repository
                    .entries(path, false)
                    .into_iter()
                    .map(|(name, node)| {
                        let full = if path.is_empty() {
                            name.clone()
                        } else {
                            format!("{path}/{name}")
                        };
                        let kind = match node {
                            None => "dir",
                            // A listing types a submodule `file`.
                            Some(GitNode::File(_) | GitNode::Submodule(_)) => "file",
                            Some(GitNode::Link(text)) => match GitRepository::resolve(&full, text)
                                .as_deref()
                                .map(|target| repository.path(target))
                            {
                                Some(GitPath::File(_)) => "file",
                                _ => "symlink",
                            },
                        };
                        serde_json::json!({
                            "name": name, "path": full, "type": kind,
                            "sha": GitRepository::blob_sha(&full)
                        })
                    })
                    .collect::<Vec<_>>();
                ResponseTemplate::new(200).set_body_json(listing)
            }
            GitPath::Through | GitPath::Missing => not_found(),
        }
    }
}

impl Respond for ServedRepository {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let path = percent_decoded(request.url.path());
        let Some((_, route)) = path.split_once(&self.prefix) else {
            return not_found();
        };
        let query = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        if let Some(reference) = route.strip_prefix("git/trees/") {
            return self.tree(reference, query("recursive").is_some());
        }
        if let Some(sha) = route.strip_prefix("git/blobs/") {
            return self.blob(sha);
        }
        let Some(file) = route
            .strip_prefix("contents")
            .map(|rest| rest.trim_start_matches('/'))
        else {
            return not_found();
        };
        if query("ref").is_some_and(|reference| reference != self.commit) {
            return not_found();
        }
        let raw = request
            .headers
            .get("accept")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|accept| accept.contains("vnd.github.raw"));
        self.contents(file, raw)
    }
}
