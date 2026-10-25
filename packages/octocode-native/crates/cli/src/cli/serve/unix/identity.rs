//! What keys a server: the build, the workspace, and the call identity
//! drawn from the configuration and the environment.

use octocode_native::config::SHELL_DIRECTORY_ENV;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The environment a call runs with on a server, and the one its identity
/// and runtime are keyed by: `env` without [`SHELL_DIRECTORY_ENV`]. The
/// call's actual directory travels separately as `cwd`. Dropping shell labels
/// also lets the server reuse its runtime when only a shell's history changes.
pub(super) fn served_env(mut env: BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.retain(|name, _| !SHELL_DIRECTORY_ENV.contains(&name.as_str()));
    env
}

/// A digest of what a server shares between the calls it answers: the
/// config module's identity, every `OCTOCODE_*` variable, and the variables
/// the engine reads from the server's own process instead of the call's
/// environment ([`PROCESS_ENV`], [`octocode_io::http::NETWORK_ENV`]).
///
/// [`PROCESS_ENV`]: octocode_engine::process_env::PROCESS_ENV
pub(super) fn call_identity(
    config: &octocode_native::config::ConfigOutput,
    env: &BTreeMap<String, String>,
) -> String {
    use std::fmt::Write as _;
    let mut identity = config.identity_digest();
    for (name, value) in env {
        let name = name.as_str();
        if octocode_io::http::NETWORK_ENV.contains(&name)
            || octocode_engine::process_env::declares(name)
            || name.starts_with("OCTOCODE_")
        {
            let _ = write!(identity, "\n{name}={value}");
        }
    }
    octocode_engine::digest::sha256(identity.as_bytes())
}

/// A digest of a whole environment: the runtimes a server keeps are told
/// apart by it, so a call never runs with another call's environment.
pub(super) fn env_digest(env: &BTreeMap<String, String>) -> String {
    let mut text = String::new();
    for (name, value) in env {
        text.push_str(name);
        text.push('\0');
        text.push_str(value);
        text.push('\0');
    }
    octocode_engine::digest::sha256(text.as_bytes())
}

/// The workspace `cwd` belongs to: its nearest ancestor holding `.git`, else
/// `cwd` itself.
pub(super) fn workspace_of(cwd: &Path) -> PathBuf {
    cwd.ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(cwd)
        .to_path_buf()
}

/// This binary by path, size and modification time: a rebuild is another
/// build.
pub(super) fn exe_identity() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let meta = std::fs::metadata(&exe).ok()?;
    let built = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |age| age.as_nanos());
    Some(format!("{}|{}|{built}", exe.display(), meta.len()))
}

/// 16 hex digits over the build, the workspace and the call identity.
pub(super) fn server_key(exe: &str, workspace: &Path, identity: &str) -> String {
    let key = format!("{exe}\n{}\n{identity}", workspace.display());
    octocode_engine::digest::sha256(key.as_bytes())[..16].to_owned()
}
