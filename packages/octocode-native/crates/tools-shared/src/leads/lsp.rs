//! The `lspSearch` lead other tools offer for a declaration they found.
use octocode_contract_types::TestCode;
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, server_command_resolves,
};
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
pub fn lead_name(row: &Value) -> String {
    match row.get("operation").and_then(Value::as_str) {
        Some("callers") => "findCallers",
        _ => "findReferences",
    }
    .to_owned()
}

/// The `lspSearch` query row for the declaration `symbol` at one-based
/// `line` of `path`, with the caller's `test_code` (both operations a lead
/// runs, `callers` and `references`, take it). `None` when no level above
/// `syntax` would answer for the file ([`level_above_syntax`]): a `syntax`
/// answer is the tree-sitter facts the producer already shows, so a lead
/// that could only repeat them, or fail, is never offered.
#[must_use]
pub fn verify_query(
    path: &str,
    symbol: &str,
    line: u64,
    verify: Verify,
    test_code: Option<TestCode>,
) -> Option<Value> {
    level_above_syntax(path).then(|| {
        let mut row = json!({
            "path": path,
            "operation": verify.operation(),
            "symbolName": symbol,
            "line": line,
        });
        if let Some(test_code) = test_code {
            row["testCode"] = json!(test_code);
        }
        row
    })
}

/// Whether the staged ladder would answer for `path` above `syntax`: an
/// editor that has the file's folder open (the IDE rung,
/// [`crate::editor_lock::editor_covers`]), or a discovered server that
/// would launch ([`server_available`]).
fn level_above_syntax(path: &str) -> bool {
    let discovery = discovery().for_one_request();
    let workspace = workspace_of(path, &discovery);
    server_available(path, &workspace, &discovery) || {
        // The rung takes canonical paths; the lead keeps the producer's.
        let canonical = std::fs::canonicalize(path).map_or_else(
            |_| path.to_owned(),
            |real| real.to_string_lossy().into_owned(),
        );
        crate::editor_lock::editor_covers(&discovery, &canonical, &workspace)
    }
}

/// The workspace a request for `path` resolves: its project root, else the
/// file's directory.
fn workspace_of(path: &str, discovery: &LspDiscoveryOptions) -> String {
    octocode_engine::lsp::config::workspace_root_for_file(path, discovery).unwrap_or_else(|_| {
        Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into())
    })
}

/// Whether the server discovered for this lead would launch: never by running
/// it. A lead hangs on a search hit, so the check costs file stats, and a rustup
/// proxy's one question is kept while what rustup reads is unchanged
/// ([`server_command_resolves`]). Installation and removal are observed
/// immediately.
fn server_available(path: &str, workspace: &str, discovery: &LspDiscoveryOptions) -> bool {
    default_server_for_file(path, workspace, discovery)
        .is_some_and(|config| server_command_resolves(&config))
}

thread_local! {
    /// Discovery the runtime resolved for the request running on this
    /// thread (see [`with_lead_discovery`]).
    static LEAD_DISCOVERY: std::cell::RefCell<Option<LspDiscoveryOptions>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `run` (one local tool row, on the calling thread) with the runtime's
/// resolved LSP settings, `options`, as the discovery every lead in it probes.
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
mod tests;
