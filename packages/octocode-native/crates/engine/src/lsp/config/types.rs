//! The server spec and discovery options every config step shares.
use crate::lsp::managed::{platform_id, resolve_cached_server};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Clone, Copy)]
pub(super) struct ServerSpec {
    pub(super) language_id: &'static str,
    pub(super) command: &'static str,
    pub(super) args: &'static [&'static str],
    /// The `workspace/configuration` section this server's
    /// `initializationOptions` stand for (rust-analyzer and gopls take the
    /// same keys under their own name); `None` = options are not a section.
    pub(super) configuration_section: Option<&'static str>,
}

#[derive(Clone, Debug, Default)]
pub struct LspDiscoveryOptions {
    pub trust_project_config: bool,
    /// The host's resolved settings (process env plus the trusted `.env`
    /// layers), never the process env directly.
    pub env: BTreeMap<String, String>,
    /// The octocode home: the user `lsp-servers.json`, managed server
    /// installs, and jdtls workspaces.
    pub octocode_home: Option<PathBuf>,
    /// The directory the host runs the call for (the runtime's working
    /// directory, not necessarily the process's): the user's own project,
    /// whose `node_modules` may supply the TypeScript server and which
    /// trusts a workspace inside it. `None` trusts no invocation directory.
    pub invocation_dir: Option<PathBuf>,
    /// Where Octocode keeps code it fetched itself, empty once the user opts
    /// in (`lsp.trustFetchedCode`). A language server can run code from the
    /// repository it opens, and nobody chose to trust this code, so discovery
    /// finds no server for a file or workspace root resolving inside one.
    pub fetched_code_roots: Vec<PathBuf>,
    /// Whether a server may keep an index under the home (`storage.mode`).
    pub index_on_disk: bool,
    /// What one request has resolved ([`Self::for_one_request`]).
    pub one_request: OneRequest,
}

/// The paths one request resolved, each once: one request asks about the same
/// file and root at every step of finding its server. Empty by default, which
/// resolves on each call.
#[derive(Clone, Debug, Default)]
pub struct OneRequest(Option<Arc<Mutex<BTreeMap<PathBuf, PathBuf>>>>);

impl OneRequest {
    fn resolved(&self, path: &Path) -> PathBuf {
        let Some(known) = &self.0 else {
            return resolved(path);
        };
        known
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(path.to_path_buf())
            .or_insert_with(|| resolved(path))
            .clone()
    }
}

/// Why `lspSearch` answers without a server on fetched code, what works there
/// instead, and how to opt in.
pub const FETCHED_CODE_REFUSAL: &str = "lspSearch starts no language server on code Octocode fetched (ghCloneRepo, ghStructure materialize): a server can run code from the repository it opens, such as its build scripts. localSearch (text) and astSearch and structureSearch (syntax) read this code without running anything, as do graphBuild and graphQuery (graph) when Octocode runs from inside this directory. A root above that code opens it too: for your own files, pass a workspaceRoot that does not hold it. To opt in, set lsp.trustFetchedCode in the home .octocoderc, or OCTOCODE_LSP_TRUST_FETCHED_CODE=true in the process environment or the home .env; a workspace cannot.";

/// `path` with symlinks resolved as far as it exists: a name no file holds yet
/// sits where its existing ancestors resolve.
fn resolved(path: &Path) -> PathBuf {
    path.ancestors()
        .find_map(|ancestor| {
            let real = std::fs::canonicalize(ancestor).ok()?;
            Some(real.join(path.strip_prefix(ancestor).ok()?))
        })
        .unwrap_or_else(|| path.to_path_buf())
}

fn holds_anything(directory: &Path) -> bool {
    std::fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_some())
}

impl LspDiscoveryOptions {
    /// The environment a server started for this call is given: the
    /// allowlisted variables of the call's own environment
    /// ([`crate::lsp::client::LSP_SERVER_ENV_ALLOWLIST`]), never the
    /// process's.
    pub fn spawn_environment(&self) -> std::collections::HashMap<String, String> {
        crate::lsp::client::LSP_SERVER_ENV_ALLOWLIST
            .iter()
            .filter_map(|name| Some(((*name).to_owned(), self.env.get(*name)?.clone())))
            .collect()
    }

    /// Where a server may keep an index: the home, unless storage is memory only.
    pub(super) fn index_home(&self) -> Option<&Path> {
        self.octocode_home.as_deref().filter(|_| self.index_on_disk)
    }

    /// Root of the managed server installs: `<octocode home>/lsp`.
    pub fn managed_root(&self) -> Option<PathBuf> {
        self.octocode_home.as_ref().map(|home| home.join("lsp"))
    }

    /// The verified managed install of `command`, when one exists.
    pub(super) fn managed_server(&self, command: &str) -> Option<PathBuf> {
        resolve_cached_server(&self.managed_root()?, command, &platform_id())
    }

    /// These options for one request, whose steps all judge what its first one
    /// saw. They must not outlive it: a root appears with the first fetch.
    #[must_use]
    pub fn for_one_request(self) -> Self {
        Self {
            one_request: OneRequest(Some(Arc::default())),
            ..self
        }
    }

    /// Whether `file` or `workspace_root` resolves inside
    /// [`Self::fetched_code_roots`], or `workspace_root` holds one with anything
    /// in it: a server enumerates its root, dot directories included (csharp-ls
    /// and jdtls ran build scripts under `.octocode/clones` from a root above it).
    /// A directory path need not sit under the root its query names, so both
    /// count. Resolved on each call, since a root appears with the first fetch,
    /// unless these options are [`Self::for_one_request`].
    pub fn opens_fetched_code(&self, file: &str, workspace_root: &str) -> bool {
        if self.fetched_code_roots.is_empty() {
            return false;
        }
        let roots = self
            .fetched_code_roots
            .iter()
            .map(|root| self.one_request.resolved(root))
            .collect::<Vec<_>>();
        let (file, workspace_root) = (
            self.one_request.resolved(Path::new(file)),
            self.one_request.resolved(Path::new(workspace_root)),
        );
        roots.iter().any(|root| {
            file.starts_with(root)
                || workspace_root.starts_with(root)
                || (root.starts_with(&workspace_root) && holds_anything(root))
        })
    }

    /// The host's resolved `lsp.trustProjectConfig` (or a trusted project).
    pub(super) fn trusts_project(&self) -> bool {
        self.trust_project_config
    }
}
