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
    use std::process::Stdio;
    let mut probe = Command::new(command);
    probe
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::process::contain(&mut probe);
    let Ok(mut child) = probe.spawn() else {
        return false;
    };
    let Ok(mut job) = crate::process::Job::attach(&child, None) else {
        let _ = child.kill();
        let _ = child.wait();
        return false;
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
    let status = child.wait();
    completed && status.is_ok_and(|status| status.success())
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
/// executable (a node-launched script must exist too). Never executes it.
pub fn server_command_resolves(config: &LspLanguageServerConfig) -> bool {
    let command = resolve_known_server_command(&config.command);
    if is_rejected_shell(&command) {
        return false;
    }
    let program = if Path::new(&command).is_absolute() || has_path_separator(&command) {
        Some(PathBuf::from(&command))
    } else {
        which::which(&command).ok()
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
    if is_rust_analyzer_command(&command) || is_rust_analyzer_command(&config.command) {
        let rustup_home = crate::process_env::var_os("RUSTUP_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::home_dir().map(|home| home.join(".rustup")));
        return rustup_proxy_component_installed(&program, rustup_home.as_deref());
    }
    true
}

/// A rustup proxy (`~/.cargo/bin/rust-analyzer -> rustup`) resolves on PATH
/// even when no toolchain ships the component; running it would install or
/// fail. Treat it as available only when some toolchain has the binary. Any
/// other executable is the real server.
pub(super) fn rustup_proxy_component_installed(program: &Path, rustup_home: Option<&Path>) -> bool {
    let target = std::fs::read_link(program).ok();
    let is_proxy = target
        .as_deref()
        .and_then(Path::file_stem)
        .is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("rustup")
        });
    if !is_proxy {
        return true;
    }
    let Some(binary) = program.file_name() else {
        return false;
    };
    let Some(toolchains) = rustup_home
        .map(|home| home.join("toolchains"))
        .and_then(|dir| std::fs::read_dir(dir).ok())
    else {
        return false;
    };
    toolchains
        .filter_map(Result::ok)
        .any(|toolchain| is_executable_path(&toolchain.path().join("bin").join(binary)))
}
