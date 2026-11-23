//! The opt-in SCIP tier of a Rust graph build: one `rust-analyzer scip` run
//! per cargo workspace and input state, its index cached on disk and shared
//! by every build of that workspace.
//!
//! The user opts in by turning build scripts and proc macros on for
//! rust-analyzer ([`scip_consent`]): the indexer then runs repository code.
//! The run uses the routed server's command and environment, nothing of the
//! process's, and ends when its caller stops (deadline or cancel), taking its
//! process tree with it.
use crate::deep_stack::caller_stopped;
use crate::digest::HashWriter;
use crate::lsp::config::{
    LspDiscoveryOptions, PROBE_POLL, ProbeError, default_server_for_file, is_rust_analyzer_command,
    probe_command_stdout, run_command_quiet, scip_consent,
};
use crate::lsp::types::LspLanguageServerConfig;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Why no SCIP index is available. Every case is a fall back to syntax-only
/// edges, never a failed build.
#[derive(Debug)]
pub enum ScipError {
    /// The routed server or its options do not consent (see [`scip_consent`]).
    NotConsented,
    /// No `Cargo.toml` at or above the root.
    NoCargoWorkspace,
    /// `<server> --version` gave no answer.
    Version(String),
    /// The input files could not be read to key the cache.
    Inputs(std::io::Error),
    /// The cache directory or its files could not be written.
    Cache(std::io::Error),
    /// The indexer failed to start or exited unsuccessfully.
    Run(String),
    /// The caller stopped (deadline or cancel) before the index was ready.
    Stopped,
    /// The indexer exited successfully but wrote no index.
    Empty,
}

impl std::fmt::Display for ScipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConsented => f.write_str(
                "SCIP indexing needs cargo.buildScripts.enable and procMacro.enable set to true for rust-analyzer",
            ),
            Self::NoCargoWorkspace => f.write_str("no Cargo.toml at or above the root"),
            Self::Version(error) => write!(f, "rust-analyzer --version failed: {error}"),
            Self::Inputs(error) => write!(f, "reading the cargo workspace failed: {error}"),
            Self::Cache(error) => write!(f, "the SCIP cache could not be written: {error}"),
            Self::Run(error) => write!(f, "rust-analyzer scip failed: {error}"),
            Self::Stopped => f.write_str("the SCIP index was not ready before the caller stopped"),
            Self::Empty => f.write_str("rust-analyzer scip wrote no index"),
        }
    }
}

impl std::error::Error for ScipError {}

/// A consented SCIP run, planned: the routed server and the cache key of its
/// inputs. Cheap next to the run itself (a `--version` and one pass over the
/// sources), so a build identity can hold [`Self::key`] before any index.
#[derive(Debug)]
pub struct ScipPlan {
    server: LspLanguageServerConfig,
    /// The cargo workspace root the indexer is run on.
    pub cargo_root: PathBuf,
    /// Hash of every input the index depends on and of the consent bit.
    pub key: String,
}

/// The cargo workspace that owns `start`: the nearest ancestor whose
/// `Cargo.toml` declares `[workspace]`, else the nearest ancestor with a
/// `Cargo.toml`.
pub fn cargo_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut nearest = None;
    for dir in start.ancestors() {
        let Ok(manifest) = std::fs::File::open(dir.join("Cargo.toml")) else {
            continue;
        };
        let declares_workspace = std::io::BufRead::lines(std::io::BufReader::new(manifest))
            .map_while(Result::ok)
            .any(|line| line.trim() == "[workspace]");
        if declares_workspace {
            return Some(dir.to_path_buf());
        }
        nearest.get_or_insert_with(|| dir.to_path_buf());
    }
    nearest
}

/// The routed rust-analyzer for `workspace_root` when the user consented to
/// indexing: build scripts and proc macros on in the routed options (the
/// project's own file counts only when trusted), and the command is
/// rust-analyzer. Fetched code routes to no server.
pub fn scip_server(
    workspace_root: &Path,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    let root = workspace_root.to_string_lossy();
    let file = workspace_root.join("x.rs");
    let server = default_server_for_file(&file.to_string_lossy(), &root, options)?;
    let consent = server
        .initialization_options
        .as_ref()
        .is_some_and(scip_consent);
    (consent && is_rust_analyzer_command(&server.command)).then_some(server)
}

/// A command for the routed server: its environment only (the call's
/// allowlisted variables and the entry's own), never the process's.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "no shared constructor for a routed server's one-shot command; its environment is cleared and set to the routed one here, and its bound is the caller's stop in probe_command"
    )
)]
fn server_command(server: &LspLanguageServerConfig) -> Command {
    let mut command = Command::new(&server.command);
    command.env_clear();
    if let Some(env) = &server.env {
        command.envs(env);
    }
    // A rustup proxy answers instead of installing a toolchain.
    command.env("RUSTUP_AUTO_INSTALL", "0");
    command
}

fn stopped_or(error: ProbeError, other: impl FnOnce(String) -> ScipError) -> ScipError {
    match error {
        ProbeError::Stopped => ScipError::Stopped,
        error => other(error.to_string()),
    }
}

/// Plan the run for the cargo workspace around `root`: `Err(NotConsented)`
/// spawns nothing.
pub fn scip_plan(root: &Path, options: &LspDiscoveryOptions) -> Result<ScipPlan, ScipError> {
    let cargo_root = cargo_workspace_root(root).ok_or(ScipError::NoCargoWorkspace)?;
    let server = scip_server(&cargo_root, options).ok_or(ScipError::NotConsented)?;
    let mut version = server_command(&server);
    version.arg("--version");
    let version = probe_command_stdout(&mut version)
        .map_err(|error| stopped_or(error, ScipError::Version))?;
    let key = scip_key(&cargo_root, &version)?;
    Ok(ScipPlan {
        server,
        cargo_root,
        key,
    })
}

/// Directories no input lives in.
const SKIPPED_DIRS: &[&str] = &["target", ".git", ".octocode", "node_modules"];

fn is_input(name: &str) -> bool {
    name == "Cargo.toml"
        || name == "Cargo.lock"
        || name.starts_with("rust-toolchain")
        || name.ends_with(".rs")
}

fn collect_inputs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ScipError> {
    if caller_stopped() {
        return Err(ScipError::Stopped);
    }
    for entry in std::fs::read_dir(dir).map_err(ScipError::Inputs)? {
        let entry = entry.map_err(ScipError::Inputs)?;
        let kind = entry.file_type().map_err(ScipError::Inputs)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_dir() && !SKIPPED_DIRS.contains(&name.as_ref()) {
            collect_inputs(&entry.path(), out)?;
        } else if kind.is_file() && is_input(&name) {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// The cache key: the digest of every `.rs` file, `Cargo.toml`, `Cargo.lock`
/// and `rust-toolchain*` file under `cargo_root` (by relative path), the
/// server's `--version` output and the consent bit (always set: a key exists
/// only for a consented run).
fn scip_key(cargo_root: &Path, version: &[u8]) -> Result<String, ScipError> {
    let mut files = Vec::new();
    collect_inputs(cargo_root, &mut files)?;
    files.sort();
    let mut hash = HashWriter::default();
    hash.update(b"scip\0consent=1\0");
    hash.update(crate::digest::sha256(version));
    for file in files {
        if caller_stopped() {
            return Err(ScipError::Stopped);
        }
        let relative = file.strip_prefix(cargo_root).unwrap_or(&file);
        hash.update(relative.to_string_lossy().as_bytes());
        hash.update(b"\0");
        // Streamed: a source file is never held whole.
        let mut content = HashWriter::default();
        let mut open = std::fs::File::open(&file).map_err(ScipError::Inputs)?;
        std::io::copy(&mut open, &mut content).map_err(ScipError::Inputs)?;
        hash.update(content.finalize());
        hash.update(b"\n");
    }
    Ok(hex::encode(hash.finalize()))
}

/// A SCIP index on disk.
#[derive(Debug, PartialEq, Eq)]
pub struct ScipFile {
    pub path: PathBuf,
    /// Another build's run (or an earlier one) produced it.
    pub reused: bool,
}

impl ScipPlan {
    /// Where the index of this plan is cached under `cache_dir`.
    pub fn cache_file(&self, cache_dir: &Path) -> PathBuf {
        cache_dir.join(format!("{}.scip", self.key))
    }

    /// The index for this plan under `cache_dir` (`<graph home>/scip/<root
    /// tag>`): a cached one for the same key, else the result of one
    /// `rust-analyzer scip` run. Builds of one workspace run the indexer once:
    /// the others wait on the lock until its run ends or their caller stops,
    /// then reuse its index. The file appears atomically (a temp file,
    /// renamed).
    pub fn index(&self, cache_dir: &Path) -> Result<ScipFile, ScipError> {
        let path = self.cache_file(cache_dir);
        if path.is_file() {
            return Ok(ScipFile { path, reused: true });
        }
        std::fs::create_dir_all(cache_dir).map_err(ScipError::Cache)?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(cache_dir.join(format!("{}.lock", self.key)))
            .map_err(ScipError::Cache)?;
        loop {
            if caller_stopped() {
                return Err(ScipError::Stopped);
            }
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    #[cfg_attr(
                        octocode_clippy_ratchet,
                        expect(
                            clippy::disallowed_methods,
                            reason = "polls an OS file lock and checks the caller's stop each time; the interval never caps the wait"
                        )
                    )]
                    std::thread::sleep(PROBE_POLL);
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(ScipError::Cache(error)),
            }
        }
        if path.is_file() {
            return Ok(ScipFile { path, reused: true });
        }
        let temp = cache_dir.join(format!("{}.{}.tmp", self.key, std::process::id()));
        let ran = self.run(&temp);
        let published = ran.and_then(|()| {
            let written = std::fs::metadata(&temp).map_err(ScipError::Cache)?;
            if written.len() == 0 {
                return Err(ScipError::Empty);
            }
            std::fs::rename(&temp, &path).map_err(ScipError::Cache)
        });
        if published.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        published?;
        prune_other_keys(cache_dir, &self.key);
        Ok(ScipFile {
            path,
            reused: false,
        })
    }

    /// `<server> scip <cargo root> --output <output>`, until it exits or the
    /// caller stops.
    fn run(&self, output: &Path) -> Result<(), ScipError> {
        let mut command = server_command(&self.server);
        command
            .arg("scip")
            .arg(&self.cargo_root)
            .arg("--output")
            .arg(output)
            .current_dir(&self.cargo_root);
        run_command_quiet(&mut command).map_err(|error| stopped_or(error, ScipError::Run))
    }
}

/// Remove the indexes of earlier input states: only the newest is read.
fn prune_other_keys(cache_dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".scip") && !name.starts_with(keep) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests;
