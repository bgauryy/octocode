//! The `lspSearch` lead other tools offer for a declaration they found.
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, is_command_available,
};
use octocode_engine::lsp::workspace::resolve_workspace_root_for_file;
use serde_json::{Value, json};
use std::path::Path;

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

/// The lead name of a [`verify_query`] row: `findCallers` or
/// `findReferences`.
pub(crate) fn lead_name(row: &Value) -> String {
    match row.get("operation").and_then(Value::as_str) {
        Some("callers") => "findCallers",
        _ => "findReferences",
    }
    .to_owned()
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
            "line": line,
        })
    })
}

/// Whether the server discovered for this lead resolves to an executable.
/// Installation and removal are observed immediately.
fn server_available(path: &str) -> bool {
    let workspace = resolve_workspace_root_for_file(path.to_owned()).unwrap_or_else(|_| {
        Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into())
    });
    let discovery = discovery();
    default_server_for_file(path, &workspace, &discovery)
        .is_some_and(|config| is_command_available(&config.command).unwrap_or(false))
}

thread_local! {
    /// Discovery the runtime resolved for the request running on this
    /// thread (see [`with_lead_discovery`]).
    static LEAD_DISCOVERY: std::cell::RefCell<Option<LspDiscoveryOptions>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `run` (one local tool row, on the calling thread) with the runtime's
/// resolved LSP settings, `options`, as the discovery every lead in it probes,
/// with no cross-request availability reuse. The caller authorizes
/// `options.config_path` against the path policy, as `lspSearch` itself does.
pub fn with_lead_discovery<R>(options: LspDiscoveryOptions, run: impl FnOnce() -> R) -> R {
    struct Restore(Option<LspDiscoveryOptions>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            LEAD_DISCOVERY.with(|cell| *cell.borrow_mut() = previous);
        }
    }
    let _restore = Restore(LEAD_DISCOVERY.with(|cell| cell.borrow_mut().replace(options)));
    run()
}

/// The runtime-resolved discovery of the current request. Every dispatched
/// local row runs inside [`with_lead_discovery`]; a caller outside one (a unit
/// test) probes the built-in defaults, never the bare process environment.
fn discovery() -> LspDiscoveryOptions {
    LEAD_DISCOVERY
        .with(|cell| cell.borrow().clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[cfg(unix)]
    #[test]
    fn each_lead_observes_server_installation_and_removal_immediately() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("directory");
        let command = dir.path().join("fixture-lsp");
        let config = dir.path().join("lsp.json");
        std::fs::write(&config, serde_json::json!({"languageServers":{".fixture":{"command":command,"args":[],"languageId":"fixture"}}}).to_string()).expect("config");
        let file = dir.path().join("source.fixture");
        std::fs::write(&file, "target").expect("source");
        let options = LspDiscoveryOptions {
            config_path: Some(config),
            ..Default::default()
        };
        with_lead_discovery(options, || {
            assert!(!server_available(file.to_str().expect("path")));
            std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
            std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
            assert!(
                server_available(file.to_str().expect("path")),
                "newly installed server is immediately discoverable"
            );
            std::fs::remove_file(&command).expect("remove server");
            assert!(
                !server_available(file.to_str().expect("path")),
                "removed server no longer produces a lead"
            );
        });
    }

    #[test]
    fn leads_probe_the_runtime_resolved_discovery_inside_a_row_only() {
        let options = LspDiscoveryOptions {
            env: BTreeMap::from([("OCTOCODE_LEAD_PROBE".to_owned(), "resolved".to_owned())]),
            ..LspDiscoveryOptions::default()
        };
        let inside = with_lead_discovery(options, discovery);
        assert_eq!(
            inside.env.get("OCTOCODE_LEAD_PROBE").map(String::as_str),
            Some("resolved")
        );
        let after = discovery();
        assert!(after.env.is_empty(), "{after:?}");
        assert!(after.config_path.is_none() && after.octocode_home.is_none());
    }

    /// A declaration lead runs `callers` for a callable and `references`
    /// otherwise, and is withheld when no language server would start for the
    /// file.
    #[cfg(unix)]
    #[test]
    fn verify_query_picks_the_operation_and_needs_a_server() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("fixture");
        // A fixture server for `.fixture`, so the test never depends on the
        // language servers installed on this machine.
        let command = dir.path().join("fixture-lsp");
        std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        let config = dir.path().join("lsp.json");
        std::fs::write(&config, serde_json::json!({"languageServers":{".fixture":{"command":command,"args":[],"languageId":"fixture"}}}).to_string()).expect("config");
        let options = LspDiscoveryOptions {
            config_path: Some(config),
            ..Default::default()
        };
        let file = dir.path().join("a.fixture");
        std::fs::write(&file, "function a() {}\n").expect("file");
        let path = file.to_string_lossy().into_owned();
        let unknown = dir.path().join("a.unknownext");
        std::fs::write(&unknown, "a\n").expect("file");
        with_lead_discovery(options, || {
            let callers =
                verify_query(&path, "a", 1, Verify::for_kind("function")).expect("fixture server");
            assert_eq!(
                callers,
                serde_json::json!({"path": path, "operation": "callers", "symbolName": "a", "line": 1})
            );
            let references =
                verify_query(&path, "A", 3, Verify::for_kind("class")).expect("fixture server");
            assert_eq!(references["operation"], "references");
            assert_eq!(
                verify_query(&unknown.to_string_lossy(), "a", 1, Verify::Callers),
                None,
                "no server for the extension: no lead"
            );
        });
    }
}
