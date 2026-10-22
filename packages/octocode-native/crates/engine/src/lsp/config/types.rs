//! The server spec and discovery options every config step shares.
use crate::lsp::managed::{platform_id, resolve_cached_server};
use std::collections::BTreeMap;
use std::path::PathBuf;

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
    pub config_path: Option<PathBuf>,
    pub trust_project_config: bool,
    /// The host's resolved settings (process env plus the trusted `.env`
    /// layers, and `lsp.cacheDir` as `OCTOCODE_LSP_CACHE_DIR`), never the
    /// process env directly.
    pub env: BTreeMap<String, String>,
    /// The octocode home: the user `lsp-servers.json`, managed server
    /// installs, and jdtls workspaces.
    pub octocode_home: Option<PathBuf>,
    /// The directory the host runs the call for (the runtime's working
    /// directory, not necessarily the process's): the user's own project,
    /// whose `node_modules` may supply the TypeScript server and which
    /// trusts a workspace inside it. `None` trusts no invocation directory.
    pub invocation_dir: Option<PathBuf>,
}

impl LspDiscoveryOptions {
    fn setting(&self, key: &str) -> Option<&str> {
        self.env
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
    }

    /// Root of the managed server installs: `OCTOCODE_LSP_CACHE_DIR`, else
    /// `<octocode home>/lsp`.
    pub fn managed_root(&self) -> Option<PathBuf> {
        self.setting("OCTOCODE_LSP_CACHE_DIR")
            .map(|dir| PathBuf::from(dir.trim()))
            .or_else(|| self.octocode_home.as_ref().map(|home| home.join("lsp")))
    }

    /// The verified managed install of `command`, when one exists.
    pub(super) fn managed_server(&self, command: &str) -> Option<PathBuf> {
        resolve_cached_server(&self.managed_root()?, command, &platform_id())
    }

    /// The host's resolved `lsp.trustProjectConfig` (or a trusted project).
    pub(super) fn trusts_project(&self) -> bool {
        self.trust_project_config
    }
}
