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

pub fn git(directory: &Path, args: &[&str]) {
    let output = fixture_git(directory, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
