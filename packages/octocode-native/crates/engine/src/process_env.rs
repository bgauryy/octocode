//! The variables the engine reads from its own process environment, not
//! from a call's. A host that keeps one process for many calls (the CLI's
//! warm server) cannot hand a call its own values of these, so it keys on
//! them. Every read goes through [`var_os`], whose name must be in
//! [`PROCESS_ENV`]; the tests below hold the engine's sources to that, so a
//! new read cannot be left out of a host's key.

use std::ffi::OsString;

/// Every variable the engine reads from its process environment.
pub const PROCESS_ENV: &[&str] = &[
    // The global ignore rules a walk applies (`search::walk`).
    "XDG_CONFIG_HOME",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    // Language-server discovery and startup share toolchain locations and
    // selection, including rustup proxies (`lsp::config`, `lsp::client`).
    "RUSTUP_HOME",
    "CARGO_HOME",
    "RUSTUP_TOOLCHAIN",
    "PATH",
    // What a spawned language server is given (`lsp::client`); `HOME` is
    // also what `std::env::home_dir` reads.
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "SYSTEMROOT",
    "SystemRoot",
    // Toolchain and scratch variables forwarded to Python, Go and JVM
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
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TEMP",
    "TMP",
];

/// Whether `name` is in [`PROCESS_ENV`].
pub const fn declares(name: &str) -> bool {
    let name = name.as_bytes();
    let mut index = 0;
    while index < PROCESS_ENV.len() {
        let declared = PROCESS_ENV[index].as_bytes();
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

/// Reads `name`, a variable in [`PROCESS_ENV`], from the process.
pub(crate) fn var_os(name: &'static str) -> Option<OsString> {
    debug_assert!(declares(name), "{name} is not in PROCESS_ENV");
    std::env::var_os(name)
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

    /// What `source` reads from the environment outside [`super::var_os`]:
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
            for (index, line) in text.lines().enumerate() {
                for read in line.split("process_env::var_os(\"").skip(1) {
                    let name = read.split('"').next().unwrap_or_default();
                    if !declares(name) {
                        undeclared.push(format!("{}:{}: {name}", path.display(), index + 1));
                    }
                }
            }
        }
        assert!(
            direct.is_empty(),
            "read through process_env::var_os: {direct:?}"
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
            "let x = crate::process_env::var_os(\"PATH\");",
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
