//! Canonical paths (memoized per request), search roots and the include
//! family of a file.
use super::*;
use crate::tools::lsp_search::paths::anchor_dir;

tokio::task_local! {
    /// Canonical forms already resolved by this request (see `canonical`).
    static CANONICAL: RefCell<HashMap<String, String>>;
}

/// Run one request with its own canonical-path memo.
pub(in super::super) async fn with_canonical_memo<F: std::future::Future>(request: F) -> F::Output {
    CANONICAL.scope(RefCell::default(), request).await
}

/// `path` with symlinks resolved; a file that is gone keeps its name under
/// its resolved directory (so it compares with live paths, e.g. a scope
/// root under a symlinked temp directory); as given when neither resolves
/// (not local). Inside [`with_canonical_memo`] each spelling is resolved
/// once per request: a server answer names the same few files hundreds of
/// times. Outside it (blocking workers, tests) every call resolves.
/// [`canonical`] of the path a `file://` URI names.
pub(in super::super) fn canonical_uri(uri: &str) -> String {
    canonical(&super::super::render::uri_to_path(uri))
}

pub(in super::super) fn canonical(path: &str) -> String {
    let resolve = || {
        let given = std::path::Path::new(path);
        std::fs::canonicalize(given)
            .or_else(|_| match (given.parent(), given.file_name()) {
                (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                    std::fs::canonicalize(parent).map(|parent| parent.join(name))
                }
                _ => Err(std::io::ErrorKind::NotFound.into()),
            })
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_owned())
    };
    CANONICAL
        .try_with(|memo| {
            if let Some(hit) = memo.borrow().get(path) {
                return hit.clone();
            }
            let resolved = resolve();
            memo.borrow_mut().insert(path.to_owned(), resolved.clone());
            resolved
        })
        .unwrap_or_else(|_| resolve())
}

/// Whether something above `anchor` says where its project ends: a repository
/// or a workspace manifest, which [`search_root`] scans to.
pub(super) fn has_project_boundary(anchor: &Path) -> bool {
    anchor_dir(anchor).ancestors().any(|dir| {
        dir.join(".git").exists()
            || octocode_engine::lsp::workspace::is_workspace_manifest_root(dir)
    })
}

/// The scope root: VCS root, else outermost workspace manifest, else the
/// server root; clamped to the policy, never the process cwd.
pub(super) fn search_root(anchor: &Path, server_root: &str, policy: &PathPolicy) -> String {
    let start = anchor_dir(anchor);
    let chosen = start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .or_else(|| {
            start
                .ancestors()
                .filter(|dir| octocode_engine::lsp::workspace::is_workspace_manifest_root(dir))
                .last()
        })
        .map_or_else(|| PathBuf::from(server_root), Path::to_path_buf);
    let authorized = |dir: &Path| {
        policy
            .validate(dir)
            .ok()
            .filter(|valid| valid.canonical.is_dir())
            .map(|valid| valid.canonical.to_string_lossy().into_owned())
    };
    // The chosen root must contain the anchor and be readable; otherwise
    // the widest authorized directory on the anchor's own path.
    if start.starts_with(&chosen)
        && let Some(root) = authorized(&chosen)
    {
        return root;
    }
    widest_authorized_dir(start, policy).unwrap_or_else(|| start.to_string_lossy().into_owned())
}

/// The request's explicit `workspace_root` (canonical) when it is an
/// authorized directory holding `anchor`; `None` otherwise, which leaves
/// the default scope root.
pub(super) fn explicit_root(
    anchor: &Path,
    workspace_root: &str,
    policy: &PathPolicy,
) -> Option<String> {
    let root = policy
        .validate(workspace_root)
        .ok()
        .filter(|valid| valid.canonical.is_dir())?
        .canonical;
    anchor
        .starts_with(&root)
        .then(|| root.to_string_lossy().into_owned())
}

/// The outermost directory of the unbroken run of authorized directories
/// from `start` upward (canonical): the widest root the policy allows on
/// `start`'s own path. `None` when `start` itself is not authorized.
pub(in super::super) fn widest_authorized_dir(start: &Path, policy: &PathPolicy) -> Option<String> {
    start
        .ancestors()
        .map_while(|dir| {
            policy
                .validate(dir)
                .ok()
                .filter(|valid| valid.canonical.is_dir())
                .map(|valid| valid.canonical.to_string_lossy().into_owned())
        })
        .last()
}

/// `*.ext` globs of the markup whose scripts `language_id`'s server cannot
/// read unaided: only JavaScript runs in a script region.
pub(super) fn markup_include(language_id: Option<&str>) -> Vec<String> {
    use octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP;
    if !octocode_engine::lsp::family::is_typescript_family(language_id) {
        return Vec::new();
    }
    SCRIPT_BEARING_MARKUP
        .iter()
        .map(|extension| format!("*.{extension}"))
        .collect()
}

/// Whether `file` is script-bearing markup by its extension.
pub(super) fn is_markup(file: &str) -> bool {
    let extension = octocode_engine::text::extension_of(file, true, "");
    octocode_engine::signatures::languages::SCRIPT_BEARING_MARKUP.contains(&extension.as_str())
}

/// `*.ext` globs of every grammar in `language_id`'s family; the anchor's
/// own extension for a language the registry does not know.
pub(super) fn family_include(language_id: Option<&str>, anchor: &Path) -> Vec<String> {
    let family: Vec<&str> = match language_id {
        Some(id) => {
            octocode_engine::lsp::family::family_of(id).map_or_else(|| vec![id], <[&str]>::to_vec)
        }
        None => Vec::new(),
    };
    let mut globs = octocode_engine::signatures::languages::all_entries()
        .iter()
        .filter(|entry| entry.language_id.is_some_and(|id| family.contains(&id)))
        .flat_map(|entry| entry.extensions.iter())
        .map(|extension| format!("*.{extension}"))
        .collect::<Vec<_>>();
    if globs.is_empty()
        && let Some(extension) = anchor.extension().filter(|_| !anchor.is_dir())
    {
        globs.push(format!("*.{}", extension.to_string_lossy()));
    }
    globs
}

impl super::Scope {
    /// The scope of a request anchored at `anchor` (a file or, for a
    /// workspace-root query, a directory), under the request's explicit
    /// `workspace_root` when it holds the anchor.
    pub(in super::super) fn resolve(
        anchor: &str,
        server_root: &str,
        workspace_root: Option<&str>,
        policy: &PathPolicy,
        language_id: Option<&str>,
    ) -> Self {
        let anchor_path = Path::new(anchor);
        let wide = search_root(anchor_path, server_root, policy);
        let explicit = workspace_root.and_then(|root| explicit_root(anchor_path, root, policy));
        let mut scope = Self::new(
            explicit.clone().unwrap_or_else(|| wide.clone()),
            family_include(language_id, anchor_path),
        );
        scope.unbounded = explicit.is_none() && !paths::has_project_boundary(anchor_path);
        scope.narrowed =
            explicit.is_some_and(|root| root != wide && Path::new(&root).starts_with(&wide));
        scope.markup = markup_include(language_id);
        scope.server_root = std::fs::canonicalize(server_root).ok();
        scope
    }
}
