//! `octocode skill` forwards to the npm launcher, which owns every skill
//! subcommand. Both entry points then share one flag set and one report.
//!
//! The launcher is the `octocode` on PATH. When that resolves to a native
//! host instead, forwarding would loop: the PATH check catches this binary
//! itself, and the delegation marker catches any other native host.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Set on the forwarded child. A host that sees it was reached by its own
/// forward, so it must stop instead of forwarding again.
const DELEGATED_ENV: &str = "OCTOCODE_SKILL_DELEGATED";

#[cfg(windows)]
const LAUNCHER_NAMES: [&str; 3] = ["octocode.cmd", "octocode.exe", "octocode"];
#[cfg(not(windows))]
const LAUNCHER_NAMES: [&str; 1] = ["octocode"];

pub fn skill(args: &[String]) -> u8 {
    let rerun = rerun_hint(args);
    if std::env::var_os(DELEGATED_ENV).is_some() {
        eprintln!(
            "octocode skill: the `octocode` on PATH is the native binary, not the npm CLI. \
             The npm launcher owns `skill`; put it first on PATH (npm i -g octocode) or run: {rerun}"
        );
        return 1;
    }
    let Some(launcher) = find_launcher(std::env::var_os("PATH").as_deref()) else {
        eprintln!(
            "octocode skill: the npm launcher (`octocode`) is not on PATH. \
             Install it (npm i -g octocode) or run: {rerun}"
        );
        return 1;
    };
    if is_current_exe(&launcher) {
        eprintln!(
            "octocode skill: the `octocode` on PATH ({}) resolves to this binary: the native binary, not the npm CLI. \
             The npm launcher owns `skill`; put it first on PATH (npm i -g octocode) or run: {rerun}",
            launcher.display()
        );
        return 1;
    }
    match Command::new(&launcher)
        .arg("skill")
        .args(args)
        .env(DELEGATED_ENV, "1")
        .status()
    {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1),
        Err(error) => {
            eprintln!(
                "octocode skill: cannot run {}: {error}. Run instead: {rerun}",
                launcher.display()
            );
            1
        }
    }
}

fn rerun_hint(args: &[String]) -> String {
    std::iter::once("npx -y octocode skill")
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

fn find_launcher(path_var: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path_var?)
        .filter(|dir| !dir.as_os_str().is_empty())
        .flat_map(|dir| LAUNCHER_NAMES.map(|name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

fn is_current_exe(candidate: &Path) -> bool {
    let Ok(current) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return false;
    };
    std::fs::canonicalize(candidate).is_ok_and(|resolved| resolved == current)
}

#[cfg(test)]
mod tests {
    use super::{find_launcher, rerun_hint};

    #[test]
    fn rerun_hint_repeats_the_argv() {
        assert_eq!(
            rerun_hint(&["list".into(), "--json".into()]),
            "npx -y octocode skill list --json"
        );
        assert_eq!(rerun_hint(&[]), "npx -y octocode skill");
    }

    #[test]
    fn launcher_lookup_skips_empty_and_missing_entries() {
        let dir = tempfile::tempdir().expect("dir");
        let name = super::LAUNCHER_NAMES[0];
        std::fs::write(dir.path().join(name), "").expect("launcher");
        let path = std::env::join_paths([
            std::path::PathBuf::new(),
            dir.path().join("missing"),
            dir.path().to_path_buf(),
        ])
        .expect("PATH");
        assert_eq!(find_launcher(Some(&path)), Some(dir.path().join(name)));
        assert_eq!(find_launcher(None), None);
    }
}
