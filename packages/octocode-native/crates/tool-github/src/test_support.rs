//! Fixtures this crate's tests share (the provider-backed runners of the
//! GitHub search tools) and the git fixtures the tests that run its tools
//! from outside it share with them.
#![cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "fixtures for tests, compiled only for them (`test`, `test-support`): they build processes, files and timings on purpose"
    )
)]
#![expect(
    clippy::expect_used,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
#[cfg(test)]
pub(crate) mod backed;

use std::path::Path;
use std::process::Command;

/// Runs fixture git in `directory`; the test fails when git does. The failure
/// names only the subcommand, never the arguments: they may carry a
/// credential (a token in a URL, the path of a planted secret). The caller's
/// line names the call.
#[track_caller]
pub fn git(directory: &Path, args: &[&str]) {
    let output = fixture_git(directory, args);
    assert!(
        output.status.success(),
        "git {} failed: {}",
        subcommand(args),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The git subcommand `args` run, spelled from this list rather than copied
/// from `args`, so no argument reaches a failure message.
fn subcommand(args: &[&str]) -> &'static str {
    const SUBCOMMANDS: [&str; 16] = [
        "add",
        "branch",
        "checkout",
        "clone",
        "commit",
        "config",
        "fetch",
        "hash-object",
        "init",
        "push",
        "remote",
        "rev-parse",
        "stash",
        "tag",
        "update-ref",
        "worktree",
    ];
    args.iter()
        .find_map(|arg| SUBCOMMANDS.into_iter().find(|known| known == arg))
        .unwrap_or("(a subcommand this list does not name)")
}

pub fn fixture_git(directory: &Path, args: &[&str]) -> std::process::Output {
    fixture_git_command(directory, args)
        .output()
        .expect("run fixture git")
}

pub fn fixture_git_command(directory: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(directory)
        .env_clear()
        .env("HOME", directory)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env(
            "GIT_CONFIG_SYSTEM",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .arg("-c")
        .arg(format!(
            "core.hooksPath={}",
            if cfg!(windows) { "NUL" } else { "/dev/null" }
        ))
        .args(args);
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command
}
