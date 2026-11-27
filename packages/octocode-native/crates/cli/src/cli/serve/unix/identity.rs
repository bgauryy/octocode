//! What keys a server: the build, the workspace and the process
//! environment. What keys a runtime inside it: the configuration identity
//! and the call's whole environment.

use octocode_runtime_base::config::SHELL_DIRECTORY_ENV;
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

/// What a runtime keeps its calls under: the configuration identity
/// ([`octocode_runtime_base::config::ConfigOutput::identity_digest`], which covers
/// every declared `OCTOCODE_*` name, both home files and the workspace's
/// rc file). It keys the runtime, not the socket: a call under another
/// configuration reaches the same server and gets its own runtime.
pub(super) fn call_identity(config: &octocode_runtime_base::config::ConfigOutput) -> String {
    config.identity_digest()
}

/// A digest of the variables the engine reads from a server's own process
/// ([`PROCESS_ENV`]) and from the environment the HTTP clients are built
/// from ([`octocode_io::http::NETWORK_ENV`]) that are set in `env`. A call
/// cannot bring its own values of these, so another value is another
/// server; the server checks it against its own ([`own_process_identity`]).
///
/// [`PROCESS_ENV`]: octocode_engine::process_env::PROCESS_ENV
pub(super) fn process_identity(env: &BTreeMap<String, String>) -> String {
    use std::fmt::Write as _;
    let mut identity = String::new();
    for (name, value) in env {
        let name = name.as_str();
        if octocode_io::http::NETWORK_ENV.contains(&name)
            || octocode_engine::process_env::declares(name)
        {
            let _ = writeln!(identity, "{name}={value}");
        }
    }
    octocode_engine::digest::sha256(identity.as_bytes())
}

/// [`process_identity`] of this process.
pub(super) fn own_process_identity() -> String {
    process_identity(&served_env(octocode_io::env::vars_lossy()))
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

/// This binary by file, version, size and modification time: the file is
/// its device and inode, so every path that reaches it (a symlink, a
/// relative or `..` spelling, a hard link) is the same build and reaches the
/// same server and its warm language servers. A copy is another file, so
/// another build with its own server. Version, size and time tell a file
/// overwritten in place from the build that started a server. No path is
/// in it: moving a binary keeps its server.
pub(super) fn exe_identity() -> Option<String> {
    build_identity(&std::env::current_exe().ok()?)
}

/// [`exe_identity`] of the binary at `exe`.
pub(super) fn build_identity(exe: &Path) -> Option<String> {
    file_identity(&std::fs::metadata(exe).ok()?, env!("CARGO_PKG_VERSION"))
}

/// The build `meta` describes, as `version` of the program.
pub(super) fn file_identity(meta: &std::fs::Metadata, version: &str) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let built = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |age| age.as_nanos());
    Some(format!(
        "{}:{}|{version}|{}|{built}",
        meta.dev(),
        meta.ino(),
        meta.len()
    ))
}

/// Salt of [`server_key`]: a change of what the key means bumps it, so a
/// new client never reaches a server of the old meaning, which idles out.
const KEY_VERSION: &str = "serve-key-2";

/// 16 hex digits over the build, the workspace and the process identity
/// ([`process_identity`]).
pub(super) fn server_key(exe: &str, workspace: &Path, process: &str) -> String {
    let key = format!("{KEY_VERSION}\n{exe}\n{}\n{process}", workspace.display());
    octocode_engine::digest::sha256(key.as_bytes())[..16].to_owned()
}
