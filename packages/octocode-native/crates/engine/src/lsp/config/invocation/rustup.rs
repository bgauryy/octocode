//! A rustup toolchain component as a server source. When the command on
//! `PATH` is a rustup proxy and the toolchain rustup selects for the workspace
//! ships that command, the proxy runs the toolchain's own build: a separate
//! official origin matched to the project's Rust version, which wins over a
//! managed download (issue #481).
use crate::lsp::commands::is_executable_path;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What rustup's selection reads, so it can be given explicitly.
#[derive(Clone, Debug, Default)]
pub(in crate::lsp::config) struct RustupView {
    pub(in crate::lsp::config) path_var: Option<OsString>,
    pub(in crate::lsp::config) rustup_home: Option<PathBuf>,
    /// `RUSTUP_TOOLCHAIN`, which overrides every toolchain file.
    pub(in crate::lsp::config) toolchain: Option<String>,
}

impl RustupView {
    /// This process's rustup environment.
    pub(in crate::lsp::config) fn current() -> Self {
        #[cfg(test)]
        if let Some(view) = TEST_VIEW.with(|view| view.borrow().clone()) {
            return view;
        }
        Self {
            path_var: crate::process_env::var_os("PATH"),
            rustup_home: crate::process_env::var_os("RUSTUP_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::home_dir().map(|home| home.join(".rustup"))),
            toolchain: crate::process_env::var_os("RUSTUP_TOOLCHAIN")
                .and_then(|value| value.into_string().ok())
                .filter(|value| !value.trim().is_empty()),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// A test's rustup environment in place of the process's.
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
    if !is_rustup_proxy(&proxy) {
        return None;
    }
    let home = view.rustup_home.as_deref()?;
    let settings = std::fs::read_to_string(home.join("settings.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    let name = view
        .toolchain
        .clone()
        .or_else(|| toolchain_file_channel(workspace_root))
        .or_else(|| directory_override(&settings, workspace_root))
        .or_else(|| {
            settings
                .get("default_toolchain")?
                .as_str()
                .map(str::to_owned)
        })?;
    let binary = proxy.file_name()?;
    toolchain_dirs(home, &settings, &name)
        .into_iter()
        .map(|dir| dir.join("bin").join(binary))
        .find(|candidate| is_executable_path(candidate))
}

/// The channel of the nearest `rust-toolchain.toml` or `rust-toolchain`
/// (TOML, or the legacy one-line form) at or above `dir`.
fn toolchain_file_channel(dir: &Path) -> Option<String> {
    dir.ancestors().find_map(|dir| {
        ["rust-toolchain.toml", "rust-toolchain"]
            .iter()
            .find_map(|name| std::fs::read_to_string(dir.join(name)).ok())
            .and_then(|text| match text.parse::<toml::Table>() {
                Ok(table) => table
                    .get("toolchain")?
                    .get("channel")?
                    .as_str()
                    .map(str::to_owned),
                Err(_) => Some(text.trim().to_owned()).filter(|line| !line.contains('\n')),
            })
    })
}

/// `rustup override set` entries: the deepest directory containing `dir`.
fn directory_override(settings: &toml::Table, dir: &Path) -> Option<String> {
    settings
        .get("overrides")?
        .as_table()?
        .iter()
        .filter(|(path, _)| dir.starts_with(path))
        .max_by_key(|(path, _)| path.len())
        .and_then(|(_, name)| name.as_str().map(str::to_owned))
}

/// Where toolchain `name` may be installed: as named, or with the host triple
/// rustup appends to a channel name (`stable` -> `stable-aarch64-apple-darwin`).
fn toolchain_dirs(home: &Path, settings: &toml::Table, name: &str) -> Vec<PathBuf> {
    let toolchains = home.join("toolchains");
    let mut dirs = vec![toolchains.join(name)];
    if let Some(host) = settings
        .get("default_host_triple")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .or_else(host_triple)
    {
        dirs.push(toolchains.join(format!("{name}-{host}")));
    }
    dirs
}

/// This build's host triple in rustup's spelling.
fn host_triple() -> Option<String> {
    let arch = std::env::consts::ARCH;
    let rest = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        "linux" if cfg!(target_env = "musl") => "unknown-linux-musl",
        "linux" => "unknown-linux-gnu",
        _ => return None,
    };
    Some(format!("{arch}-{rest}"))
}
