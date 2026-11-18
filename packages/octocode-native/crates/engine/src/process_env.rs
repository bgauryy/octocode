//! The environment the engine reads, and from where.
//!
//! - [`CALL_ENV`]: variables a call carries. A host that keeps one process
//!   for many calls (the CLI's warm server, shared by MCP sessions) hands each
//!   call its own values (`LspDiscoveryOptions::env`, the spawn environment
//!   of `LspLanguageServerConfig::env`), and the engine reads them only
//!   through [`call_var`], never from its own process: a call's language
//!   servers are found and started under the caller's `PATH` and toolchain
//!   variables.
//! - [`PROCESS_ENV`]: variables read from the engine's own process, which no
//!   call can bring: `HOME` (`std::env::home_dir`). The global ignore rules of
//!   a walk are the call's own ([`GlobalIgnore`]), never the process's. A host that
//!   keeps one process for many calls keys on them. The tests below hold the engine's
//!   sources to both lists: a direct read of the process environment is
//!   refused, and `home_dir` needs `HOME` and `USERPROFILE` declared.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;

mod global_ignore;

pub use global_ignore::GlobalIgnore;

/// Every variable the engine reads from its process environment.
pub const PROCESS_ENV: &[&str] = &[
    // What `std::env::home_dir` reads (discovery's per-user locations).
    "HOME",
    "USERPROFILE",
];

/// Every variable the engine reads from a call's environment.
pub const CALL_ENV: &[&str] = &[
    // Language-server discovery and startup share toolchain locations and
    // selection, including rustup proxies (`lsp::config`, `lsp::client`).
    "RUSTUP_HOME",
    "CARGO_HOME",
    "RUSTUP_TOOLCHAIN",
    "PATH",
    // What a spawned language server is given (`lsp::client`).
    "XDG_CONFIG_HOME",
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "SYSTEMROOT",
    "SystemRoot",
    // The git config files that name the global ignore file a call's walks
    // apply (`search::walk::GlobalIgnore`), beside `XDG_CONFIG_HOME` and
    // `HOME` above.
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    // Toolchain and scratch variables forwarded to Python, Go, JVM and .NET
    // language servers (`lsp::client`).
    "VIRTUAL_ENV",
    "CONDA_PREFIX",
    "PYTHONPATH",
    "GOPATH",
    "GOMODCACHE",
    "GOFLAGS",
    "GOWORK",
    "GOPROXY",
    "GOPRIVATE",
    "GONOSUMDB",
    "JAVA_HOME",
    "DOTNET_ROOT",
    "DOTNET_ROOT_ARM",
    "DOTNET_ROOT_ARM64",
    "DOTNET_ROOT_X64",
    "DOTNET_ROOT_X86",
    "DOTNET_ROOT(x86)",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TEMP",
    "TMP",
];

/// A call's environment: the host's resolved variables.
pub type Env = BTreeMap<String, String>;

/// A map of variable names to values the engine can read a call's
/// environment from.
pub trait EnvMap {
    fn lookup(&self, name: &str) -> Option<&String>;
}

impl EnvMap for BTreeMap<String, String> {
    fn lookup(&self, name: &str) -> Option<&String> {
        self.get(name)
    }
}

impl EnvMap for HashMap<String, String> {
    fn lookup(&self, name: &str) -> Option<&String> {
        self.get(name)
    }
}

/// Whether a request carries `name` to the engine: a setting (`OCTOCODE_*`)
/// or a [`CALL_ENV`] variable.
pub fn carried(name: &str) -> bool {
    name.starts_with("OCTOCODE_") || call_declares(name)
}

/// Whether `name` is in [`CALL_ENV`].
pub const fn call_declares(name: &str) -> bool {
    listed(CALL_ENV, name)
}

/// Reads `name`, a variable in [`CALL_ENV`], from the call's `env`: never
/// from the process.
pub fn call_var(env: &impl EnvMap, name: &'static str) -> Option<OsString> {
    debug_assert!(call_declares(name), "{name} is not in CALL_ENV");
    env.lookup(name).map(OsString::from)
}

/// Whether `list` holds `name`, as a constant expression.
const fn listed(list: &[&str], name: &str) -> bool {
    let name = name.as_bytes();
    let mut index = 0;
    while index < list.len() {
        let declared = list[index].as_bytes();
        if declared.len() == name.len() {
            let mut at = 0;
            while at < name.len() && declared[at] == name[at] {
                at += 1;
            }
            if at == name.len() {
                return true;
            }
        }
        index += 1;
    }
    false
}

/// Whether `name` is in [`PROCESS_ENV`].
pub const fn declares(name: &str) -> bool {
    listed(PROCESS_ENV, name)
}

#[cfg(test)]
mod tests;
