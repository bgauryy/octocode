use super::invocation::{
    current_node_command, is_node_executable, is_rust_analyzer_command,
    resolve_known_server_command, typescript_cli_from_command,
};
use crate::lsp::commands::{has_path_separator, is_executable_path, is_rejected_shell};
use crate::lsp::types::LspLanguageServerConfig;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Why a probe gave no successful answer.
#[derive(Debug)]
pub(crate) enum ProbeError {
    /// The command could not start.
    Spawn(std::io::Error),
    /// The started command could not be placed in its process job; it was
    /// killed.
    Contain(String),
    /// The caller stopped (its deadline passed or it was cancelled) before
    /// the command exited; its process tree was killed.
    Stopped,
    /// Waiting for the command failed.
    Wait(std::io::Error),
    /// The command ran and exited unsuccessfully: its answer is "no".
    Exited(std::process::ExitStatus),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "the probe could not start: {error}"),
            Self::Contain(error) => write!(f, "the probe could not be contained: {error}"),
            Self::Stopped => f.write_str("the probe was stopped before it answered"),
            Self::Wait(error) => write!(f, "waiting for the probe failed: {error}"),
            Self::Exited(status) => write!(f, "the probe exited unsuccessfully ({status})"),
        }
    }
}

/// Run `command args..` with null stdio; `Ok` when it exited successfully.
/// See [`probe_command`]. Blocking: call from a blocking thread.
#[cfg(test)]
pub(crate) fn probe_command_succeeds(command: &str, args: &[&str]) -> Result<(), ProbeError> {
    let mut probe = Command::new(command);
    probe.args(args);
    probe_command(&mut probe, false).map(drop)
}

/// Run a prepared `probe` (its stdin and stderr are nulled) and return its
/// stdout when it exited successfully. The probe has no clock of its own: it
/// runs until it exits or its caller stops ([`crate::deep_stack::while_stoppable`],
/// raised at the request's deadline or cancel), and then its whole process
/// tree is killed ([`ProbeError::Stopped`]). Blocking. The output is read
/// after the child exits, so it must be small: a command that fills the pipe
/// waits for its caller to stop.
pub(crate) fn probe_command_stdout(probe: &mut Command) -> Result<Vec<u8>, ProbeError> {
    probe_command(probe, true)
}

fn probe_command(probe: &mut Command, capture_stdout: bool) -> Result<Vec<u8>, ProbeError> {
    use std::io::Read;
    use std::process::Stdio;
    probe
        .stdin(Stdio::null())
        .stdout(if capture_stdout {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(Stdio::null());
    crate::process::contain(probe);
    let mut child = probe.spawn().map_err(ProbeError::Spawn)?;
    let mut job = match crate::process::Job::attach(&child, None) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProbeError::Contain(error.to_string()));
        }
    };
    let waited = loop {
        #[cfg(unix)]
        let exited = crate::process::exited_without_reaping(child.id());
        #[cfg(not(unix))]
        let exited = child.try_wait().map(|status| status.is_some());
        match exited {
            Ok(true) => break Ok(()),
            Ok(false) if crate::deep_stack::caller_stopped() => break Err(ProbeError::Stopped),
            Ok(false) => std::thread::sleep(PROBE_POLL),
            Err(error) => break Err(ProbeError::Wait(error)),
        }
    };
    crate::process::kill_tree(child.id());
    job.terminate();
    if waited.is_err() {
        let _ = child.kill();
    }
    let mut stdout = Vec::new();
    if waited.is_ok()
        && let Some(mut pipe) = child.stdout.take()
    {
        pipe.read_to_end(&mut stdout).map_err(ProbeError::Wait)?;
    }
    let status = child.wait().map_err(ProbeError::Wait);
    waited?;
    let status = status?;
    if status.success() {
        Ok(stdout)
    } else {
        Err(ProbeError::Exited(status))
    }
}

/// How often a running probe checks whether it exited or its caller stopped.
const PROBE_POLL: std::time::Duration = std::time::Duration::from_millis(10);

/// Whether `command` resolves to a usable language-server executable. A
/// rust-analyzer is asked `--version` with `RUSTUP_AUTO_INSTALL=0`, so a rustup
/// proxy answers instead of installing a toolchain; a probe that could not
/// answer (stopped, or failed for another reason than a missing or
/// non-executable program) is an error, not "unavailable".
pub fn is_command_available(command: &str) -> Result<bool, String> {
    let command = resolve_known_server_command(command);
    if is_rejected_shell(&command) {
        return Ok(false);
    }
    if typescript_cli_from_command(&command).is_some() {
        return Ok(current_node_command().is_some());
    }
    if is_rust_analyzer_command(&command) {
        let mut probe = Command::new(&command);
        probe.arg("--version").env("RUSTUP_AUTO_INSTALL", "0");
        return match probe_command(&mut probe, false) {
            Ok(_) => Ok(true),
            Err(ProbeError::Exited(_)) => Ok(false),
            Err(ProbeError::Spawn(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(format!("{command}: {error}")),
        };
    }
    if command
        == std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    {
        return Ok(true);
    }
    if Path::new(&command).is_absolute() {
        return Ok(is_executable_path(Path::new(&command)));
    }
    which::which(&command)
        .map(|path| is_executable_path(&path))
        .or_else(|err| match err {
            which::Error::CannotFindBinaryPath => Ok(false),
            other => Err(other.to_string()),
        })
}

/// Whether a discovered server config would launch: its program is an
/// executable (a node-launched script must exist too). Never runs the server;
/// a rustup proxy costs one bounded `rustup which` call, kept until something
/// rustup reads changes.
pub fn server_command_resolves(config: &LspLanguageServerConfig) -> bool {
    let command = resolve_known_server_command(&config.command);
    if is_rejected_shell(&command) {
        return false;
    }
    let view = super::invocation::rustup::RustupView::current();
    let program = if Path::new(&command).is_absolute() || has_path_separator(&command) {
        Some(PathBuf::from(&command))
    } else {
        which::which_in(&command, view.path_var.as_ref(), &config.workspace_root).ok()
    };
    let Some(program) = program.filter(|path| is_executable_path(path)) else {
        return false;
    };
    if is_node_executable(&program)
        && let Some(script) = config.args.as_ref().and_then(|args| args.first())
        && script.ends_with(".mjs")
        && !Path::new(script).is_file()
    {
        return false;
    }
    if (is_rust_analyzer_command(&command) || is_rust_analyzer_command(&config.command))
        && super::invocation::rustup::is_rustup_proxy(&program)
    {
        // A rustup proxy resolves on PATH even when the toolchain rustup
        // selects has no such component; running it would install or fail.
        // rustup decides, not this code: available only when it names the binary.
        return super::invocation::rustup::selected_component(
            &view,
            &program,
            Path::new(&config.workspace_root),
        )
        .is_some();
    }
    true
}

/// Why `config`'s server must not start: the rustup proxy would run the
/// toolchain a workspace toolchain file names by `path`, a program the
/// repository chose. `None` for any other launch.
pub fn repository_toolchain_refusal(config: &LspLanguageServerConfig) -> Option<String> {
    if !is_rust_analyzer_command(&config.command) {
        return None;
    }
    let file = super::invocation::rustup::repository_toolchain(
        &super::invocation::rustup::RustupView::current(),
        Path::new(&config.workspace_root),
        &config.command,
    )?;
    Some(format!(
        "rust-analyzer was not started: {} names a toolchain by path, and the rustup proxy would run that directory's rust-analyzer, a program from the repository. To run it anyway, name it as the server command in ~/.octocode/lsp-servers.json; use localSearch or localFetch meanwhile.",
        file.display()
    ))
}
