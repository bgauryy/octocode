//! The `lspSearch` lead other tools offer for a declaration they found.
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, is_command_available,
};
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// What a lead asks about the declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verify {
    /// Its call sites, each with the enclosing caller.
    Callers,
    /// Every reference, calls or not.
    References,
}

impl Verify {
    /// `Callers` for a callable declaration kind, `References` otherwise.
    #[must_use]
    pub fn for_kind(kind: &str) -> Self {
        match kind {
            "function" | "method" | "fn" | "constructor" | "def" | "func" => Self::Callers,
            _ => Self::References,
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Self::Callers => "callers",
            Self::References => "references",
        }
    }
}

/// The `lspSearch` query row for the declaration `symbol` at one-based
/// `line` of `path`. `None` when no language server would start for the
/// file, so a lead that could only fail is never offered.
#[must_use]
pub fn verify_query(path: &str, symbol: &str, line: u64, verify: Verify) -> Option<Value> {
    server_available(path).then(|| {
        json!({
            "path": path,
            "operation": verify.operation(),
            "symbolName": symbol,
            "lineHint": line,
        })
    })
}

/// Whether the server discovered for `path` resolves to an executable, once
/// per extension, workspace and discovery settings. A failed probe is not
/// cached, so the next lead asks again.
fn server_available(path: &str) -> bool {
    /// `(extension, workspace, discovery settings hash)`.
    type ProbeKey = (String, String, u64);
    static KNOWN: OnceLock<Mutex<HashMap<ProbeKey, bool>>> = OnceLock::new();
    let extension = Path::new(path)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let workspace = resolve_workspace_root_for_file(path.to_owned()).unwrap_or_else(|_| {
        Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into())
    });
    let discovery = discovery();
    let settings = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{discovery:?}").hash(&mut hasher);
        hasher.finish()
    };
    let key = (extension, workspace, settings);
    let known = KNOWN.get_or_init(Mutex::default);
    if let Some(available) = known.lock().ok().and_then(|map| map.get(&key).copied()) {
        return available;
    }
    let available = match default_server_for_file(path, &key.1, &discovery) {
        Some(config) => match is_command_available(&config.command) {
            Ok(available) => available,
            Err(_) => return false,
        },
        None => false,
    };
    if let Ok(mut map) = known.lock() {
        map.insert(key, available);
    }
    available
}

thread_local! {
    /// Discovery the runtime resolved for the request running on this
    /// thread (see [`with_lead_discovery`]).
    static LEAD_DISCOVERY: std::cell::RefCell<Option<LspDiscoveryOptions>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `run` (one local tool row, on the calling thread) with the runtime's
/// resolved LSP settings as the discovery every lead in it probes; the
/// configured server file is used only when the path policy authorizes it,
/// as `lspSearch` itself does.
pub fn with_lead_discovery<R>(
    execution: &super::LspExecutionConfig,
    paths: &crate::policy::path::PathPolicy,
    run: impl FnOnce() -> R,
) -> R {
    struct Restore(Option<LspDiscoveryOptions>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            LEAD_DISCOVERY.with(|cell| *cell.borrow_mut() = previous);
        }
    }
    let config_path = execution
        .config_path
        .as_deref()
        .and_then(|path| paths.validate_read(path).ok())
        .map(|validated| validated.canonical);
    let options = execution.discovery(config_path);
    let _restore = Restore(LEAD_DISCOVERY.with(|cell| cell.borrow_mut().replace(options)));
    run()
}

/// The runtime-resolved discovery of the current request, else (a caller
/// outside a dispatched row, such as a unit test) the process environment.
fn discovery() -> LspDiscoveryOptions {
    if let Some(options) = LEAD_DISCOVERY.with(|cell| cell.borrow().clone()) {
        return options;
    }
    let env: BTreeMap<String, String> = std::env::vars()
        .filter(|(key, _)| key.starts_with("OCTOCODE_"))
        .collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let os_home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    LspDiscoveryOptions {
        config_path: env
            .get("OCTOCODE_LSP_CONFIG")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from),
        trust_project_config: false,
        octocode_home: Some(crate::config::octocode_home(&env, &cwd, &os_home)),
        env,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::path::{PathPolicy, PathPolicyConfig};

    #[test]
    fn leads_probe_the_runtime_resolved_discovery_inside_a_row_only() {
        let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");
        let execution = super::super::LspExecutionConfig {
            env: BTreeMap::from([("OCTOCODE_LEAD_PROBE".to_owned(), "resolved".to_owned())]),
            config_path: Some("/nonexistent/octocode-lsp.json".to_owned()),
            ..Default::default()
        };
        let inside = with_lead_discovery(&execution, &paths, discovery);
        assert_eq!(
            inside.env.get("OCTOCODE_LEAD_PROBE").map(String::as_str),
            Some("resolved")
        );
        // An unauthorized (here missing) server file is never used.
        assert_eq!(inside.config_path, None);
        let after = discovery();
        assert_ne!(
            after.env.get("OCTOCODE_LEAD_PROBE").map(String::as_str),
            Some("resolved")
        );
    }
}
