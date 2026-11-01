//! Canonical paths (memoized per request), search roots and the include
//! family of a file.
use super::*;

tokio::task_local! {
    /// Canonical forms already resolved by this request (see `canonical`).
    static CANONICAL: RefCell<HashMap<String, String>>;
}

/// Run one request with its own canonical-path memo.
pub(in super::super) async fn with_canonical_memo<F: std::future::Future>(request: F) -> F::Output {
    CANONICAL.scope(RefCell::default(), request).await
}

/// `path` with symlinks resolved, or as given when it cannot be (gone, or
/// not local). Inside [`with_canonical_memo`] each spelling is resolved
/// once per request: a server answer names the same few files hundreds of
/// times. Outside it (blocking workers, tests) every call resolves.
/// [`canonical`] of the path a `file://` URI names.
pub(in super::super) fn canonical_uri(uri: &str) -> String {
    canonical(&super::super::render::uri_to_path(uri))
}

pub(in super::super) fn canonical(path: &str) -> String {
    let resolve = || {
        std::fs::canonicalize(path)
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

/// The scope root: VCS root, else outermost workspace manifest, else the
/// server root; clamped to the policy, never the process cwd.
pub(super) fn search_root(anchor: &Path, server_root: &str, policy: &PathPolicy) -> String {
    let start = if anchor.is_dir() {
        anchor
    } else {
        anchor.parent().unwrap_or(anchor)
    };
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
