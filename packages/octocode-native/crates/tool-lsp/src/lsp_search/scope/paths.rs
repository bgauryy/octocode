//! Canonical paths (memoized per request), search roots and the include
//! family of a file.
use super::*;
use crate::lsp_search::paths::anchor_dir;

tokio::task_local! {
    /// Canonical forms already resolved by this request (see `canonical`).
    static CANONICAL: RefCell<HashMap<String, String>>;
}

tokio::task_local! {
    /// The sources this request judged whole, and the texts it showed of
    /// them (see [`judged_files`]).
    static JUDGED: std::sync::Arc<octocode_runtime_base::security::JudgedFiles>;
}

/// Run one request whose rows show their sources as `files` judges them.
pub(in super::super) async fn with_judged_files<F: std::future::Future>(
    files: std::sync::Arc<octocode_runtime_base::security::JudgedFiles>,
    request: F,
) -> F::Output {
    JUDGED.scope(files, request).await
}

/// The sources this request judged whole: inside [`with_judged_files`] the
/// request's own, so a file many rows quote is judged once and the output
/// gate honors what they show. Outside it (blocking workers, tests) a new
/// one, whose texts the gate judges as any other leaf.
pub(in super::super) fn judged_files()
-> std::sync::Arc<octocode_runtime_base::security::JudgedFiles> {
    JUDGED.try_with(std::sync::Arc::clone).unwrap_or_default()
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

/// `path` resolved through its symlinks, or as given when it does not
/// resolve: a server or project root compared with another resolved one.
pub(in super::super) fn canonical_root(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.into())
}

/// The heap bytes one kept path weighs.
pub(super) fn path_bytes(path: &Path) -> usize {
    std::mem::size_of::<PathBuf>() + path.as_os_str().len()
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

/// The project directories (canonical, sorted) in the parent of `root`: its
/// children, other than `root`, that hold a project marker themselves and a
/// file of the language family `include` names (a sibling of another language
/// is no project the anchor's server could have loaded). With `name`, only a
/// project with a file of the family that spells it as a word: one that never
/// does cannot use it, so no lead or note names it. One directory is
/// listed, and only what the read policy authorizes. `stopped` ends the walks
/// early (the request's cancellation or deadline); the list is then incomplete.
pub(super) fn sibling_projects(
    root: &Path,
    policy: &PathPolicy,
    include: &[String],
    name: Option<&str>,
    stopped: &dyn Fn() -> bool,
) -> Vec<String> {
    let authorized = |dir: &Path| {
        policy
            .validate(dir)
            .ok()
            .filter(|valid| valid.canonical.is_dir())
            .map(|valid| valid.canonical)
    };
    let Some(parent) = root.parent().and_then(authorized) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&parent) else {
        return Vec::new();
    };
    let mut found = entries
        .filter_map(Result::ok)
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path())
        .filter(|path| path != root && path.is_dir())
        .filter(|path| {
            octocode_engine::lsp::workspace::project_root_of_dir(path).as_deref() == Some(path)
        })
        .filter_map(|path| authorized(&path))
        .filter(|path| {
            !stopped() && holds_file_of(path, (include, name), policy.global_ignore(), stopped)
        })
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// Whether `dir` holds, under the ignore rules the text scans honor, a file
/// matching one of the `include` globs (and, with a name, spelling it as a
/// word); the walk stops at the first. No glob means no language to ask:
/// every project qualifies.
fn holds_file_of(
    dir: &Path,
    (include, name): (&[String], Option<&str>),
    global: octocode_engine::search::walk::GlobalIgnore,
    stopped: &dyn Fn() -> bool,
) -> bool {
    if include.is_empty() {
        return true;
    }
    let mut globs = globset::GlobSetBuilder::new();
    for glob in include {
        match globset::Glob::new(glob) {
            Ok(glob) => {
                globs.add(glob);
            }
            Err(_) => return true,
        }
    }
    let Ok(globs) = globs.build() else {
        return true;
    };
    octocode_tools_shared::pruned_walk(dir, [], global)
        .build()
        .take_while(|_| !stopped())
        .filter_map(Result::ok)
        .any(|entry| {
            entry.file_type().is_some_and(|kind| kind.is_file())
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|file| globs.is_match(file))
                && name.is_none_or(|name| spells_word(entry.path(), name))
        })
}

/// Whether the file at `path` spells `name` as a word. A file that cannot be
/// read whole counts as spelling it: nothing says it does not.
fn spells_word(path: &Path, name: &str) -> bool {
    let Ok(bytes) = octocode_tools_shared::source::read_bounded(
        path,
        octocode_runtime_base::security::SourceLimits::REAL.whole_source,
    ) else {
        return true;
    };
    let text = String::from_utf8_lossy(&bytes);
    let word = |byte: Option<&u8>| byte.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    text.match_indices(name).any(|(at, _)| {
        !word(text.as_bytes()[..at].last()) && !word(text.as_bytes().get(at + name.len()))
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
        discovery: &octocode_engine::lsp::config::LspDiscoveryOptions,
    ) -> Self {
        let anchor_path = Path::new(anchor);
        let wide = search_root(anchor_path, server_root, policy);
        let explicit = workspace_root.and_then(|root| explicit_root(anchor_path, root, policy));
        let mut scope = Self::new(
            explicit.clone().unwrap_or_else(|| wide.clone()),
            family_include(language_id, anchor_path),
        );
        scope.unbounded = explicit.is_none() && !paths::has_project_boundary(anchor_path);
        if scope.unbounded {
            // The projects beside the root are looked for only when a note
            // names them ([`Self::resolve_siblings`]).
            scope.policy = Some(policy.clone());
        }
        scope.narrowed =
            explicit.is_some_and(|root| root != wide && Path::new(&root).starts_with(&wide));
        scope.global_ignore = policy.global_ignore();
        scope.markup = markup_include(language_id);
        scope.server_root = std::fs::canonicalize(server_root).ok();
        scope.discovery = discovery.clone();
        scope
    }

    /// Follow what the request's server loads. rust-analyzer whose effective
    /// options (the `rustContext` overlay applied) turn build scripts off
    /// loads no file of the Cargo cache directories it leaves out
    /// ([`octocode_engine::lsp::workspace::UnloadedCargoCaches`]), so the
    /// scope walk does not enter them. Its own default runs build scripts:
    /// only an explicit `false` turns them off.
    pub(in super::super) fn serve_with(
        mut self,
        config: &octocode_engine::lsp::types::LspLanguageServerConfig,
    ) -> Self {
        self.cargo_caches_unloaded = config.language_id.as_deref() == Some("rust")
            && config.initialization_options.as_ref().and_then(|options| {
                octocode_engine::lsp::lookup_section(options, "cargo.buildScripts.enable")
            }) == Some(&serde_json::Value::Bool(false));
        self
    }

    /// The language server root of `file` as this request's discovery finds
    /// it ([`octocode_engine::lsp::config::workspace_root_for_file`]).
    pub(in super::super) fn project_root_of(
        &self,
        file: &str,
    ) -> octocode_engine::error::Result<String> {
        octocode_engine::lsp::config::workspace_root_for_file(file, &self.discovery)
    }

    /// Find the projects beside an unbounded scope's root, once (those that
    /// spell `name`, when it is known): the walk
    /// runs here, when a note names them, not for every request. A walk
    /// `stopped` ends is not kept, so a later call finds them whole.
    pub(in super::super) fn resolve_siblings(
        &self,
        name: Option<&str>,
        stopped: &dyn Fn() -> bool,
    ) {
        let Some(policy) = self.policy.as_ref().filter(|_| self.unbounded) else {
            return;
        };
        if self.siblings.get().is_some() {
            return;
        }
        let found = sibling_projects(Path::new(&self.root), policy, &self.include, name, stopped);
        if !stopped() {
            let _ = self.siblings.set(found);
        }
    }

    /// Whether a walk for the projects beside the root finished: a walk the
    /// request stopped is not kept, so an empty [`Self::siblings`] proves no
    /// absence until this holds.
    pub(in super::super) fn siblings_searched(&self) -> bool {
        self.siblings.get().is_some()
    }

    /// The projects beside the root ([`Self::resolve_siblings`]); empty until
    /// they are found.
    pub(in super::super) fn siblings(&self) -> &[String] {
        self.siblings.get().map_or(&[], Vec::as_slice)
    }
}
