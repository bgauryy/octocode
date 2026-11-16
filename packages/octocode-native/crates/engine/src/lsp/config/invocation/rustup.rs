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
use crate::lsp::config::kept::{Kept, follow_path_search, kept};
use crate::lsp::config::probe::{ProbeError, probe_command_stdout};
use crate::process_env::EnvMap;
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
    /// What `env`, a call's environment, gives rustup: its `PATH`, home and
    /// toolchain selection.
    pub(in crate::lsp::config) fn of(env: &impl EnvMap) -> Self {
        #[cfg(test)]
        if let Some(view) = TEST_VIEW.with(|view| view.borrow().clone()) {
            return view;
        }
        Self {
            path_var: crate::process_env::call_var(env, "PATH"),
            home: crate::process_env::call_var(env, "RUSTUP_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::home_dir().map(|home| home.join(".rustup"))),
            toolchain: crate::process_env::call_var(env, "RUSTUP_TOOLCHAIN"),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// A test's `PATH` in place of the process's.
    static TEST_VIEW: std::cell::RefCell<Option<RustupView>> = const { std::cell::RefCell::new(None) };
}

/// The call environment that gives discovery `view`: its `PATH`, rustup home
/// and toolchain selection.
#[cfg(test)]
pub(in crate::lsp::config) fn view_env(
    view: &RustupView,
) -> std::collections::BTreeMap<String, String> {
    [
        ("PATH", view.path_var.clone()),
        ("RUSTUP_HOME", view.home.clone().map(OsString::from)),
        ("RUSTUP_TOOLCHAIN", view.toolchain.clone()),
    ]
    .into_iter()
    .filter_map(|(name, value)| Some((name.to_owned(), value?.to_string_lossy().into_owned())))
    .collect()
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
/// in `workspace_root`; `Ok(None)` when `command` is not a rustup proxy there
/// or the selected toolchain does not ship it, the probe error when rustup
/// gave no answer ([`selected_component`]).
pub(in crate::lsp::config) fn toolchain_component(
    view: &RustupView,
    workspace_root: &Path,
    command: &str,
) -> Result<Option<PathBuf>, ProbeError> {
    let Ok(proxy) = which::which_in(command, view.path_var.as_ref(), workspace_root) else {
        return Ok(None);
    };
    selected_component(view, &proxy, workspace_root)
}

/// The binary the rustup `proxy` (`~/.cargo/bin/rust-analyzer -> rustup`)
/// would run for `workspace_root`, as `rustup which` reports it; `Ok(None)`
/// when `proxy` is not a rustup proxy or the selected toolchain lacks the
/// binary or is not installed. A kept answer whose inputs ([`inputs_of`]) still
/// hold is served without a process.
///
/// rustup runs in the workspace root, where it reads overrides and toolchain
/// files, with `RUSTUP_AUTO_INSTALL=0`: rustup 1.29 otherwise installs a
/// missing toolchain (a download) to answer. It runs until it answers or its
/// caller stops ([`probe_command_stdout`]); an unanswered question is the
/// probe's error and is not kept, so the next call asks again, and so does
/// whatever kept an answer built on it (the route). Blocking.
pub(in crate::lsp::config) fn selected_component(
    view: &RustupView,
    proxy: &Path,
    workspace_root: &Path,
) -> Result<Option<PathBuf>, ProbeError> {
    if !is_rustup_proxy(proxy) {
        return Ok(None);
    }
    let Some(rustup) = rustup_binary(proxy) else {
        return Ok(None);
    };
    let key = Question {
        proxy: proxy.to_path_buf(),
        workspace: workspace_root.to_path_buf(),
        home: view.home.clone(),
        toolchain: view.toolchain.clone(),
    };
    if let Some(kept) = ANSWERS.get(&key).filter(|kept| kept.value.inputs.holds()) {
        ANSWERS.touch(&key, crate::memory::IDLE);
        return Ok(kept.value.binary.clone());
    }
    let started = settle_clock();
    // Read before rustup runs: a change while it runs reads as a change.
    let inputs = inputs_of(view, proxy, &rustup, workspace_root);
    let binary = ask_rustup(view, &rustup, proxy, workspace_root)?.filter(|binary| {
        view.toolchain.is_some() || naming_toolchain_file(workspace_root, Some(binary)).is_none()
    });
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
    Ok(binary)
}

/// rustup's answer: `Ok(Some)` the binary it names, `Ok(None)` that the
/// selected toolchain has none (a failure exit, or no executable named), or
/// the probe error when rustup gave no answer.
fn ask_rustup(
    view: &RustupView,
    rustup: &Path,
    proxy: &Path,
    workspace_root: &Path,
) -> Result<Option<PathBuf>, ProbeError> {
    let Some(name) = proxy.file_name() else {
        return Ok(None);
    };
    let mut which = Command::new(rustup);
    which
        .arg("which")
        .arg(name)
        .current_dir(workspace_root)
        .env("RUSTUP_AUTO_INSTALL", "0");
    // rustup answers under the call's environment, not the process's.
    for (name, value) in [
        ("PATH", &view.path_var),
        ("RUSTUP_TOOLCHAIN", &view.toolchain),
        ("RUSTUP_HOME", &view.home.clone().map(OsString::from)),
    ] {
        match value {
            Some(value) => which.env(name, value),
            None => which.env_remove(name),
        };
    }
    let stdout = match probe_command_stdout(&mut which) {
        Ok(stdout) => stdout,
        Err(ProbeError::Exited(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(String::from_utf8(stdout)
        .ok()
        .map(|text| PathBuf::from(text.trim()))
        .filter(|binary| is_executable_path(binary)))
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
    let mut inputs = Proof::default();
    inputs.link(proxy.to_path_buf());
    inputs.follow(rustup.to_path_buf());
    follow_selection(&mut inputs, view, workspace)?;
    Some(inputs)
}

/// Stamp what selects a toolchain and its binaries for `workspace`: rustup's
/// settings, the set of toolchains and each one's binaries, and a toolchain
/// file in the workspace or any directory above it. `None` when rustup's home
/// is unknown or the workspace cannot be resolved.
pub(in crate::lsp::config) fn follow_selection(
    inputs: &mut Proof,
    view: &RustupView,
    workspace: &Path,
) -> Option<()> {
    let home = view.home.as_ref()?;
    let toolchains = home.join("toolchains");
    let workspace = std::fs::canonicalize(workspace).ok()?;
    inputs.follow(home.join("settings.toml"));
    for toolchain in std::fs::read_dir(&toolchains)
        .into_iter()
        .flatten()
        .flatten()
    {
        inputs.follow(toolchain.path().join("bin"));
    }
    inputs.follow(toolchains);
    follow_toolchain_files(inputs, &workspace);
    Some(())
}

/// Stamp each toolchain file rustup would read in `workspace` or above it.
fn follow_toolchain_files(inputs: &mut Proof, workspace: &Path) {
    for dir in workspace.ancestors() {
        for name in TOOLCHAIN_FILES {
            inputs.follow(dir.join(name));
        }
    }
}

/// Answers kept this process, against the one process budget, each until
/// [`crate::memory::IDLE`] after its last read.
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
    ANSWERS.insert(question, answer, weight, Some(crate::memory::IDLE));
}

/// Whether `binary` lies in a toolchain that a toolchain file in `workspace` or
/// above it names by `path`: a directory the repository chose, so the server
/// would be the repository's own program. rustup accepts only an absolute path
/// there (a checkout at a predictable place, such as CI's). `RUSTUP_TOOLCHAIN`
/// selects ahead of any file, so the caller asks only without it. A user who
/// wants such a build names it as the server command in `lsp-servers.json`.
/// The toolchain file that names it; with no `binary`, any file naming a path.
fn naming_toolchain_file(workspace: &Path, binary: Option<&Path>) -> Option<PathBuf> {
    let resolved = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let binary = binary.map(resolved);
    workspace.ancestors().find_map(|dir| {
        TOOLCHAIN_FILES
            .iter()
            .map(|name| dir.join(name))
            .find(|file| {
                crate::lsp::config_file::read_utf8(file)
                    .ok()
                    .and_then(|text| toolchain_path(&text))
                    .is_some_and(|path| {
                        binary
                            .as_ref()
                            .is_none_or(|binary| binary.starts_with(resolved(&dir.join(path))))
                    })
            })
    })
}

/// The toolchain file whose `path` the rustup proxy `command` would run in
/// `workspace_root` ([`naming_toolchain_file`]); `None` when `command` is no
/// rustup proxy there, or `RUSTUP_TOOLCHAIN` selects ahead of the file. Kept
/// while each `PATH` directory (where the proxy is found and linked) and each
/// toolchain file of the workspace and above it is unchanged.
pub(in crate::lsp::config) fn repository_toolchain(
    view: &RustupView,
    workspace_root: &Path,
    command: &str,
) -> Option<PathBuf> {
    if view.toolchain.is_some() {
        return None;
    }
    let question = NamingQuestion {
        workspace: workspace_root.to_path_buf(),
        command: command.to_owned(),
        path_var: view.path_var.clone(),
    };
    let inputs = || {
        let mut inputs = Proof::default();
        follow_toolchain_files(&mut inputs, workspace_root);
        Some(inputs)
    };
    let answer = || {
        let proxy = which::which_in(command, view.path_var.as_ref(), workspace_root).ok();
        let file = proxy
            .as_deref()
            .filter(|proxy| is_rustup_proxy(proxy))
            .and_then(|_| naming_toolchain_file(workspace_root, None));
        Naming { proxy, file }
    };
    let searched = |naming: &Naming| {
        let mut inputs = Proof::default();
        follow_path_search(
            &mut inputs,
            view.path_var.as_deref(),
            &[command],
            naming.proxy.as_deref(),
        );
        Some(inputs)
    };
    kept(&NAMING, question, inputs, answer, searched).file
}

/// A [`repository_toolchain`] answer: the proxy `which` found (how far its
/// `PATH` search went) and the toolchain file that names a path.
#[derive(Clone, Debug)]
struct Naming {
    proxy: Option<PathBuf>,
    file: Option<PathBuf>,
}

/// What a [`repository_toolchain`] answer is an answer to.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct NamingQuestion {
    workspace: PathBuf,
    command: String,
    path_var: Option<OsString>,
}

static NAMING: Memo<NamingQuestion, Kept<Naming>> = Memo::new(&crate::memory::MEMORY);

/// The `path` a toolchain file names: `[toolchain] path` in TOML, or a legacy
/// one-line file whose line is a path.
fn toolchain_path(text: &str) -> Option<PathBuf> {
    if let Ok(parsed) = text.parse::<toml::Table>() {
        return parsed
            .get("toolchain")?
            .get("path")?
            .as_str()
            .map(PathBuf::from);
    }
    let line = text.trim();
    (Path::new(line).is_absolute()).then(|| PathBuf::from(line))
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
