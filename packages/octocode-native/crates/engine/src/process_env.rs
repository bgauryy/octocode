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
mod tests {
    use super::{PROCESS_ENV, declares};
    use std::path::{Path, PathBuf};

    /// The engine's production sources: test and bench modules read their
    /// own variables and never run in a host. A `tests/` directory holds the
    /// children of a `tests.rs` module.
    fn production_sources() -> Vec<(PathBuf, String)> {
        let mut files = Vec::new();
        let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("src dir") {
                let path = entry.expect("entry").path();
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                if path.is_dir() {
                    if name != "tests" {
                        dirs.push(path);
                    }
                } else if name.ends_with(".rs")
                    && !name.ends_with("tests.rs")
                    && !name.ends_with("_bench.rs")
                    && !name.starts_with("test_")
                    && name != "process_env.rs"
                {
                    let text = std::fs::read_to_string(&path).expect("source");
                    files.push((path, text));
                }
            }
        }
        files
    }

    /// The `std::env` functions that read the process environment, and the
    /// variables `home_dir` reads.
    const DIRECT_READS: [&str; 4] = ["var", "var_os", "vars", "vars_os"];
    const HOME_DIR_READS: [&str; 2] = ["HOME", "USERPROFILE"];

    /// What `source` reads from the environment through `std::env` (`home_dir` aside, the engine reads none):
    /// `(line, read)` for each direct call (`env::var_os(`), each `use` of a
    /// read function, a glob or a renamed `std::env`, after which a bare call
    /// reads, and each `home_dir`.
    fn env_reads(source: &str) -> Vec<(usize, String)> {
        let mut reads = Vec::new();
        let lines: Vec<&str> = source.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let first = index;
            let line = lines[index].trim_start();
            let declaration = line
                .split_once("use ")
                .filter(|(head, _)| ["", "pub ", "pub(crate) ", "pub(super) "].contains(head));
            if declaration.is_some() {
                let mut statement = String::new();
                while index < lines.len() {
                    statement.push_str(lines[index]);
                    index += 1;
                    if statement.contains(';') {
                        break;
                    }
                }
                let compact: String = statement.split_whitespace().collect();
                let names: Vec<&str> = statement
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .filter(|name| !name.is_empty())
                    .collect();
                if names.contains(&"env") {
                    let renamed = names.windows(2).any(|pair| pair == ["env", "as"]);
                    let glob = compact.contains("env::*");
                    if renamed || glob || names.iter().any(|name| DIRECT_READS.contains(name)) {
                        reads.push((first + 1, compact));
                    } else if names.contains(&"home_dir") {
                        reads.push((first + 1, "home_dir".to_owned()));
                    }
                }
                continue;
            }
            let elsewhere = lines[index].replace("process_env::", "");
            for read in DIRECT_READS.iter().chain(&["home_dir"]) {
                if elsewhere.contains(&format!("env::{read}(")) {
                    reads.push((index + 1, (*read).to_owned()));
                }
            }
            index += 1;
        }
        reads
    }

    #[test]
    fn every_process_variable_is_read_through_the_declared_list() {
        let mut direct = Vec::new();
        let mut undeclared = Vec::new();
        for (path, text) in production_sources() {
            for (line, read) in env_reads(&text) {
                let at = format!("{}:{line}", path.display());
                if read == "home_dir" {
                    // `home_dir` reads `HOME` (`USERPROFILE` on Windows).
                    undeclared.extend(
                        HOME_DIR_READS
                            .iter()
                            .filter(|name| !declares(name))
                            .map(|name| format!("{at}: {name}")),
                    );
                } else {
                    direct.push(format!("{at}: {read}"));
                }
            }
            // A call variable is read from a call's environment, by a name
            // in CALL_ENV.
            for (index, line) in text.lines().enumerate() {
                for read in line.split("call_var(").skip(1) {
                    let call = read.split(')').next().unwrap_or_default();
                    if let Some(name) = call.split('"').nth(1)
                        && !super::call_declares(name)
                    {
                        undeclared.push(format!("{}:{}: {name}", path.display(), index + 1));
                    }
                }
            }
        }
        assert!(
            direct.is_empty(),
            "read the process environment directly: {direct:?}"
        );
        assert!(undeclared.is_empty(), "add to PROCESS_ENV: {undeclared:?}");
    }

    /// The scan catches each way a source can read a variable directly.
    #[test]
    fn the_source_scan_catches_every_direct_read_form() {
        for source in [
            "let x = std::env::var_os(\"X\");",
            "let x = env::var(\"X\");",
            "use std::env::var_os;\nfn f() { var_os(\"X\"); }",
            "use std::env::{self, var};",
            "use env::var;",
            "use std::{\n    env::{args, vars_os},\n    fs,\n};",
            "use std::env::*;",
            "use std::env as environment;",
            "pub(crate) use std::env::vars;",
        ] {
            assert!(
                env_reads(source).iter().any(|(_, read)| read != "home_dir"),
                "{source}"
            );
        }
        for source in [
            "let home = std::env::home_dir();",
            "use std::env::home_dir;",
        ] {
            assert!(!env_reads(source).is_empty(), "{source}");
        }
        for source in [
            "use std::env;\nlet dir = env::current_dir();",
            "use std::env::consts::OS;",
            "use std::{env, collections::*};",
            "use crate::config::vars;",
            "// reuse std::env::var naming",
        ] {
            assert!(
                env_reads(source).is_empty(),
                "{source}: {:?}",
                env_reads(source)
            );
        }
    }

    /// A name a call carries is not also read from the process, except the
    /// ones a spawned server is handed from the call (`HOME` and friends are
    /// both: discovery reads the process's, a server is given the call's).
    #[test]
    fn a_name_read_from_the_process_is_never_the_only_source_of_a_call_variable() {
        for name in [
            "PATH",
            "LANG",
            "TMPDIR",
            "RUSTUP_HOME",
            "GOPATH",
            "XDG_CONFIG_HOME",
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_SYSTEM",
        ] {
            assert!(super::call_declares(name), "{name}");
            assert!(!declares(name), "{name} must not key a server");
        }
    }

    /// Reads come from the call's map, not the process.
    #[test]
    fn a_call_variable_is_read_from_the_call_env_only() {
        let env = super::Env::from([("PATH".to_owned(), "/call/bin".to_owned())]);
        assert_eq!(
            super::call_var(&env, "PATH"),
            Some(std::ffi::OsString::from("/call/bin"))
        );
        assert_eq!(super::call_var(&super::Env::new(), "PATH"), None);
    }

    #[test]
    fn declares_matches_exact_names_only() {
        for name in PROCESS_ENV {
            assert!(declares(name), "{name}");
        }
        assert!(!declares("XDG_CONFIG"));
        assert!(!declares("XDG_CONFIG_HOMES"));
        assert!(!declares("xdg_config_home"));
        assert!(!declares(""));
    }
}
