//! Commands the npm launcher owns (`skill`, `config view`): this host forwards
//! them, verbatim, to the `octocode` launcher on PATH.
//!
//! When that `octocode` resolves to a native host instead, forwarding would
//! loop: the PATH check catches this binary itself, and the delegation marker
//! catches any other native host.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
const LAUNCHER_NAMES: [&str; 3] = ["octocode.cmd", "octocode.exe", "octocode"];
#[cfg(not(windows))]
const LAUNCHER_NAMES: [&str; 1] = ["octocode"];

/// One launcher-owned command.
pub(super) struct Forward {
    /// The command words, for example `["config", "view"]`.
    pub command: &'static [&'static str],
    /// Set on the forwarded child. A host that sees it was reached by its own
    /// forward, so it must stop instead of forwarding again.
    pub marker: &'static str,
    /// The exit code when the launcher cannot run.
    pub failure_exit: u8,
}

pub(super) const SKILL: Forward = Forward {
    command: &["skill"],
    marker: "OCTOCODE_SKILL_DELEGATED",
    failure_exit: 1,
};

pub(super) const CONFIG_VIEW: Forward = Forward {
    command: &["config", "view"],
    marker: "OCTOCODE_CONFIG_VIEW_DELEGATED",
    failure_exit: 5,
};

impl Forward {
    /// Run the launcher with `args` after the command words and return its
    /// exit code, or explain on stderr why it cannot run.
    pub(super) fn run(&self, args: &[String]) -> u8 {
        let name = self.command.join(" ");
        let rerun = rerun_hint(self.command, args);
        let fail = |reason: String| {
            eprintln!("octocode {name}: {reason}");
            self.failure_exit
        };
        let native_on_path = format!(
            "the native binary, not the npm CLI. The npm launcher owns `{name}`; \
             put it first on PATH (npm i -g octocode) or run: {rerun}"
        );
        if std::env::var_os(self.marker).is_some() {
            return fail(format!("the `octocode` on PATH is {native_on_path}"));
        }
        let Some(launcher) = find_launcher(std::env::var_os("PATH").as_deref()) else {
            return fail(format!(
                "the npm launcher (`octocode`) is not on PATH. \
                 Install it (npm i -g octocode) or run: {rerun}"
            ));
        };
        if is_current_exe(&launcher) {
            return fail(format!(
                "the `octocode` on PATH ({}) resolves to this binary: {native_on_path}",
                launcher.display()
            ));
        }
        match Command::new(&launcher)
            .args(self.command)
            .args(args)
            .env(self.marker, "1")
            .status()
        {
            Ok(status) => status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(self.failure_exit),
            Err(error) => fail(format!(
                "cannot run {}: {error}. Run instead: {rerun}",
                launcher.display()
            )),
        }
    }
}

fn rerun_hint(command: &[&str], args: &[String]) -> String {
    ["npx", "-y", "octocode"]
        .into_iter()
        .chain(command.iter().copied())
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
    use super::{CONFIG_VIEW, SKILL, find_launcher, rerun_hint};

    #[test]
    fn rerun_hint_repeats_the_argv() {
        assert_eq!(
            rerun_hint(SKILL.command, &["list".into(), "--json".into()]),
            "npx -y octocode skill list --json"
        );
        assert_eq!(rerun_hint(SKILL.command, &[]), "npx -y octocode skill");
        assert_eq!(
            rerun_hint(CONFIG_VIEW.command, &["--no-open".into()]),
            "npx -y octocode config view --no-open"
        );
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
