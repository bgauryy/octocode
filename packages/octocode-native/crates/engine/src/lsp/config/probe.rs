use super::invocation::{
    current_node_command, is_node_executable, is_rust_analyzer_command,
    resolve_known_server_command, typescript_cli_from_command,
};
use crate::lsp::commands::{has_path_separator, is_executable_path, is_rejected_shell};
use crate::lsp::types::LspLanguageServerConfig;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Upper bound for an availability probe that must execute the command (a
/// rustup proxy can otherwise block for minutes installing a toolchain).
pub(crate) const COMMAND_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Run `command args..` with null stdio and report whether it exited
/// successfully within `timeout`. On timeout the child is killed and reaped and
/// the probe reports `false`. Blocking: call from a blocking thread, never from
/// an async executor or the Node main thread when avoidable.
pub(crate) fn probe_command_succeeds(
    command: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> bool {
    let mut probe = Command::new(command);
    probe.args(args);
    run_bounded(&mut probe, false, timeout).is_some()
}

/// Run a prepared `probe` (its stdin and stderr are nulled) and return its
/// stdout when it exited successfully within `timeout`; `None` on a failure
/// exit, a timeout (the child is killed) or a spawn error. Blocking, like
/// [`probe_command_succeeds`]. The output is read after the child exits, so
/// it must be small: a command that fills the pipe times out.
pub(crate) fn probe_command_stdout(
    probe: &mut Command,
    timeout: std::time::Duration,
) -> Option<Vec<u8>> {
    run_bounded(probe, true, timeout)
}

fn run_bounded(
    probe: &mut Command,
    capture_stdout: bool,
    timeout: std::time::Duration,
) -> Option<Vec<u8>> {
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
    let mut child = probe.spawn().ok()?;
    let Ok(mut job) = crate::process::Job::attach(&child, None) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let deadline = std::time::Instant::now() + timeout;
    let completed = loop {
        #[cfg(unix)]
        let exited = crate::process::exited_without_reaping(child.id());
        #[cfg(not(unix))]
        let exited = child.try_wait().map(|status| status.is_some());
        match exited {
            Ok(true) => break true,
            Ok(false) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => break false,
        }
    };
    crate::process::kill_tree(child.id());
    job.terminate();
    if !completed {
        let _ = child.kill();
    }
    let mut stdout = Vec::new();
    if completed && let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_end(&mut stdout);
    }
    let status = child.wait();
    (completed && status.is_ok_and(|status| status.success())).then_some(stdout)
}

/// Whether `command` resolves to a usable language-server executable. Bounded
/// by [`COMMAND_PROBE_TIMEOUT`] when the check has to execute the command.
pub fn is_command_available(command: &str) -> Result<bool, String> {
    let command = resolve_known_server_command(command);
    if is_rejected_shell(&command) {
        return Ok(false);
    }
    if typescript_cli_from_command(&command).is_some() {
        return Ok(current_node_command().is_some());
    }
    if is_rust_analyzer_command(&command) {
        return Ok(probe_command_succeeds(
            &command,
            &["--version"],
            COMMAND_PROBE_TIMEOUT,
        ));
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
