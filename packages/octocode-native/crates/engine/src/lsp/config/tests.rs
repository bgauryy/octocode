use super::invocation::rustup::RustupView;
use super::{
    LspDiscoveryOptions, default_server_for_directory, default_server_for_file, detect_language_id,
    directory_languages, directory_representative, invocation::is_node_executable,
    invocation::resolve_known_server_command, is_rust_analyzer_command,
};
#[cfg(unix)]
use super::{
    current_node_command, invocation::resolve_server_invocation,
    invocation::resolve_server_invocation_with_environment, is_command_available,
    serves_requests_concurrently,
};
#[cfg(unix)]
use crate::lsp::commands::command_resolves_to_executable;
use crate::lsp::types::{LspLanguageServerConfig, ServerSource};
use crate::test_support::TmpDir;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

thread_local! {
    /// A test's `PATH` in place of the process's.
    pub(super) static TEST_VIEW: std::cell::RefCell<Option<RustupView>> = const { std::cell::RefCell::new(None) };
}

/// The call environment that gives discovery `view`: its `PATH`, rustup home
/// and toolchain selection.
pub(super) fn view_env(view: &RustupView) -> BTreeMap<String, String> {
    [
        ("PATH", view.path_var.clone()),
        (
            "RUSTUP_HOME",
            view.home.clone().map(std::ffi::OsString::from),
        ),
        ("RUSTUP_TOOLCHAIN", view.toolchain.clone()),
    ]
    .into_iter()
    .filter_map(|(name, value)| Some((name.to_owned(), value?.to_string_lossy().into_owned())))
    .collect()
}

/// Run `body` with `view` as this thread's rustup environment.
pub(super) fn with_view<T>(view: RustupView, body: impl FnOnce() -> T) -> T {
    TEST_VIEW.with(|slot| *slot.borrow_mut() = Some(view));
    let result = body();
    TEST_VIEW.with(|slot| *slot.borrow_mut() = None);
    result
}

fn write_lsp_config(root: &std::path::Path, json: &str) {
    let config_dir = root.join(".octocode");
    std::fs::create_dir_all(&config_dir).expect("create .octocode dir");
    std::fs::write(config_dir.join("lsp-servers.json"), json).expect("write lsp config");
}

mod call_environment;
mod catalog;
mod discovery;
#[cfg(unix)]
mod fetched_code;
mod invocation;
#[cfg(unix)]
mod probe;
mod repository_code;
mod resolution;
mod route_memo;
#[cfg(unix)]
mod rustup_fixture;
#[cfg(unix)]
mod rustup_memo;
#[cfg(unix)]
mod rustup_toolchain_path;
mod scip_consent;
mod server_defaults;
mod tsgo;
mod workspace_root;

/// Each routed extension has one built-in server: no extension is listed
/// under two catalog entries.
#[test]
fn each_routed_extension_has_one_route() {
    let mut extensions = super::catalog::ROUTED_EXTENSIONS.clone();
    extensions.sort_unstable();
    extensions.dedup();
    assert_eq!(extensions.len(), super::catalog::ROUTED_EXTENSIONS.len());
}

/// The call environment of a test that looks programs up on the test
/// process's own `PATH`.
pub(super) fn process_path() -> std::collections::HashMap<String, String> {
    std::env::var("PATH")
        .map(|path| std::collections::HashMap::from([("PATH".to_owned(), path)]))
        .unwrap_or_default()
}
