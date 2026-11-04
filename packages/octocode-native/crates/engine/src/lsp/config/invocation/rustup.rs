//! A rustup toolchain component as a server source. When the command on
//! `PATH` is a rustup proxy and the toolchain rustup selects for the workspace
//! ships that command, the proxy runs the toolchain's own build: a separate
//! official origin matched to the project's Rust version, which wins over a
//! managed download (issue #481). Which toolchain that is, rustup decides:
//! `rustup which` applies its own precedence (`RUSTUP_TOOLCHAIN`, directory
//! overrides, toolchain files, the default), so none of it is re-implemented.
//!
//! Its answer costs a process, and every lead and `lspSearch` request asks: it
//! is kept per proxy, workspace and environment with the stamps of everything
//! rustup reads to give it ([`inputs_of`]), and asked again as soon as one differs.
use crate::lsp::commands::is_executable_path;
use crate::lsp::config::probe::{COMMAND_PROBE_TIMEOUT, probe_command_stdout};
use crate::stamp::{Proof, settle_clock};
use octocode_io::memo::Memo;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What finding the proxy reads, so it can be given explicitly.
#[derive(Clone, Debug, Default)]
pub(in crate::lsp::config) struct RustupView {
    pub(in crate::lsp::config) path_var: Option<OsString>,
    /// rustup's home: where its settings and toolchains live.
    pub(in crate::lsp::config) home: Option<PathBuf>,
    /// `RUSTUP_TOOLCHAIN`, which selects a toolchain over everything else.
    pub(in crate::lsp::config) toolchain: Option<OsString>,
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
            home: crate::process_env::var_os("RUSTUP_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::home_dir().map(|home| home.join(".rustup"))),
            toolchain: crate::process_env::var_os("RUSTUP_TOOLCHAIN"),
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
    selected_component(view, &proxy, workspace_root)
}

/// The binary the rustup `proxy` (`~/.cargo/bin/rust-analyzer -> rustup`)
/// would run for `workspace_root`, as `rustup which` reports it; `None` when
/// `proxy` is not a rustup proxy or the selected toolchain lacks the binary
/// or is not installed. A kept answer whose inputs ([`inputs_of`]) still hold is served
/// without a process.
///
/// rustup runs in the workspace root, where it reads overrides and toolchain
/// files, with `RUSTUP_AUTO_INSTALL=0`: rustup 1.29 otherwise installs a
/// missing toolchain (a download) to answer. Bounded by
/// [`COMMAND_PROBE_TIMEOUT`]; blocking.
pub(in crate::lsp::config) fn selected_component(
    view: &RustupView,
    proxy: &Path,
    workspace_root: &Path,
) -> Option<PathBuf> {
    if !is_rustup_proxy(proxy) {
        return None;
    }
    let rustup = rustup_binary(proxy)?;
    let key = Question {
        proxy: proxy.to_path_buf(),
        workspace: workspace_root.to_path_buf(),
        home: view.home.clone(),
        toolchain: view.toolchain.clone(),
    };
    if let Some(kept) = ANSWERS.get(&key).filter(|kept| kept.value.inputs.holds()) {
        return kept.value.binary.clone();
    }
    let started = settle_clock();
    // Read before rustup runs: a change while it runs reads as a change.
    let inputs = inputs_of(view, proxy, &rustup, workspace_root);
    let binary = ask_rustup(&rustup, proxy, workspace_root);
    if let Some(mut inputs) = inputs {
        if let Some(binary) = &binary {
            inputs.follow(binary.clone());
        }
        if inputs.settled_before(started) {
            keep_answer(
                key,
                Answer {
                    inputs,
                    binary: binary.clone(),
                },
            );
        }
    }
    binary
}

fn ask_rustup(rustup: &Path, proxy: &Path, workspace_root: &Path) -> Option<PathBuf> {
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

/// What one answer of rustup is an answer to.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Question {
    proxy: PathBuf,
    workspace: PathBuf,
    home: Option<PathBuf>,
    toolchain: Option<OsString>,
}

struct Answer {
    inputs: Proof,
    binary: Option<PathBuf>,
}

/// Toolchain files rustup looks for, in each directory from the workspace up.
const TOOLCHAIN_FILES: [&str; 2] = ["rust-toolchain", "rust-toolchain.toml"];

/// Everything `rustup which` reads to answer: the proxy link and the rustup it
/// runs, its settings (the default toolchain and directory overrides), the set
/// of toolchains and each one's binaries, and a toolchain file in the workspace
/// or any directory above it. `None` when rustup's home is unknown or the
/// workspace cannot be resolved: what rustup reads cannot be named, so nothing
/// proves an answer later.
fn inputs_of(view: &RustupView, proxy: &Path, rustup: &Path, workspace: &Path) -> Option<Proof> {
    let home = view.home.as_ref()?;
    let toolchains = home.join("toolchains");
    let workspace = std::fs::canonicalize(workspace).ok()?;
    let mut inputs = Proof::default();
    inputs.link(proxy.to_path_buf());
    inputs.follow(rustup.to_path_buf());
    inputs.follow(home.join("settings.toml"));
    for toolchain in std::fs::read_dir(&toolchains)
        .into_iter()
        .flatten()
        .flatten()
    {
        inputs.follow(toolchain.path().join("bin"));
    }
    inputs.follow(toolchains);
    for dir in workspace.ancestors() {
        for name in TOOLCHAIN_FILES {
            inputs.follow(dir.join(name));
        }
    }
    Some(inputs)
}

/// Answers kept this process, against the one process budget.
static ANSWERS: Memo<Question, Answer> = Memo::new(&crate::memory::MEMORY);

fn keep_answer(question: Question, answer: Answer) {
    let weight = std::mem::size_of::<Question>()
        + [
            Some(question.proxy.as_os_str()),
            Some(question.workspace.as_os_str()),
            question.home.as_ref().map(|home| home.as_os_str()),
            question.toolchain.as_deref(),
            answer.binary.as_ref().map(|binary| binary.as_os_str()),
        ]
        .into_iter()
        .flatten()
        .map(std::ffi::OsStr::len)
        .sum::<usize>()
        + answer.inputs.weight();
    ANSWERS.insert(question, answer, weight, None);
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
