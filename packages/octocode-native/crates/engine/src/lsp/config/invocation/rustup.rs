//! A rustup toolchain component as a server source. When the command on
//! `PATH` is a rustup proxy and the toolchain rustup selects for the workspace
//! ships that command, the proxy runs the toolchain's own build: a separate
//! official origin matched to the project's Rust version, which wins over a
//! managed download (issue #481). Which toolchain that is, rustup decides:
//! `rustup which` applies its own precedence (`RUSTUP_TOOLCHAIN`, directory
//! overrides, toolchain files, the default), so none of it is re-implemented.
use crate::lsp::commands::is_executable_path;
use crate::lsp::config::probe::{COMMAND_PROBE_TIMEOUT, probe_command_stdout};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What finding the proxy reads, so it can be given explicitly.
#[derive(Clone, Debug, Default)]
pub(in crate::lsp::config) struct RustupView {
    pub(in crate::lsp::config) path_var: Option<OsString>,
}

impl RustupView {
    /// This process's `PATH`.
    pub(in crate::lsp::config) fn current() -> Self {
        #[cfg(test)]
        if let Some(view) = TEST_VIEW.with(|view| view.borrow().clone()) {
            return view;
        }
        Self {
            path_var: crate::process_env::var_os("PATH"),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// A test's `PATH` in place of the process's.
    static TEST_VIEW: std::cell::RefCell<Option<RustupView>> = const { std::cell::RefCell::new(None) };
}

/// Run `body` with `view` as this thread's rustup environment.
#[cfg(test)]
pub(in crate::lsp::config) fn with_view<T>(view: RustupView, body: impl FnOnce() -> T) -> T {
    TEST_VIEW.with(|slot| *slot.borrow_mut() = Some(view));
    let result = body();
    TEST_VIEW.with(|slot| *slot.borrow_mut() = None);
    result
}

/// True when a link's target names rustup (`~/.cargo/bin/x -> rustup`).
pub(in crate::lsp::config) fn is_rustup_proxy(program: &Path) -> bool {
    std::fs::read_link(program)
        .ok()
        .as_deref()
        .and_then(Path::file_stem)
        .is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("rustup")
        })
}

/// The toolchain build of `command` that the rustup proxy on `PATH` would run
/// in `workspace_root`; `None` when `command` is not a rustup proxy there or
/// the selected toolchain does not ship it.
pub(in crate::lsp::config) fn toolchain_component(
    view: &RustupView,
    workspace_root: &Path,
    command: &str,
) -> Option<PathBuf> {
    let proxy = which::which_in(command, view.path_var.as_ref(), workspace_root).ok()?;
    selected_component(&proxy, workspace_root)
}

/// The binary the rustup `proxy` (`~/.cargo/bin/rust-analyzer -> rustup`)
/// would run for `workspace_root`, as `rustup which` reports it; `None` when
/// `proxy` is not a rustup proxy or the selected toolchain lacks the binary
/// or is not installed.
///
/// rustup runs in the workspace root, where it reads overrides and toolchain
/// files, with `RUSTUP_AUTO_INSTALL=0`: rustup 1.29 otherwise installs a
/// missing toolchain (a download) to answer. Bounded by
/// [`COMMAND_PROBE_TIMEOUT`]; blocking.
pub(in crate::lsp::config) fn selected_component(
    proxy: &Path,
    workspace_root: &Path,
) -> Option<PathBuf> {
    if !is_rustup_proxy(proxy) {
        return None;
    }
    let rustup = rustup_binary(proxy)?;
    let mut which = Command::new(rustup);
    which
        .arg("which")
        .arg(proxy.file_name()?)
        .current_dir(workspace_root)
        .env("RUSTUP_AUTO_INSTALL", "0");
    let stdout = probe_command_stdout(&mut which, COMMAND_PROBE_TIMEOUT)?;
    let binary = PathBuf::from(String::from_utf8(stdout).ok()?.trim());
    is_executable_path(&binary).then_some(binary)
}

/// The `rustup` a proxy dispatches to: its link target (a bare name is a
/// sibling in the proxy's directory), else a sibling `rustup`.
fn rustup_binary(proxy: &Path) -> Option<PathBuf> {
    let dir = proxy.parent()?;
    let target = std::fs::read_link(proxy).ok()?;
    [dir.join(target), dir.join("rustup")]
        .into_iter()
        .find(|candidate| is_executable_path(candidate))
}
