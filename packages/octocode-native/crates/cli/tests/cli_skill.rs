//! The npm launcher owns `octocode skill`. The host binary forwards every
//! skill invocation to the `octocode` launcher on PATH, verbatim, and refuses
//! to forward when that would reach a native host again.
// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![cfg(unix)]

use crate::support;

use std::path::{Path, PathBuf};
use std::process::Output;
use support::Workspace;

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap_or("")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap_or("")
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// A stand-in npm launcher: records argv and the delegation marker, prints a
/// fixed report, and exits with a fixed code.
fn stub_launcher(workspace: &Workspace) -> (PathBuf, PathBuf) {
    let bin = workspace.home.join("launcher-bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    let log = workspace.home.join("launcher-argv.txt");
    let stub = bin.join("octocode");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\n\
             {{ printf 'marker=%s\\n' \"$OCTOCODE_SKILL_DELEGATED\"; for a in \"$@\"; do printf 'arg=%s\\n' \"$a\"; done; }} >> '{}'\n\
             printf '{{\"report\":\"node\"}}\\n'\n\
             exit 4\n",
            log.display()
        ),
    )
    .expect("stub");
    make_executable(&stub);
    (bin, log)
}

#[test]
fn every_skill_subcommand_is_forwarded_verbatim_to_the_launcher() {
    let workspace = Workspace::new();
    let (bin, log) = stub_launcher(&workspace);
    let cases: [&[&str]; 9] = [
        &[],
        &["list", "--json"],
        &["list", "--all"],
        &["install", "demo", "--platform", "claude", "--global"],
        &["remove", "demo", "--purge"],
        &["check", "--fix", "--workspace"],
        &["info", "octocode-research"],
        &["sync"],
        &["list", "--unknown-flag"],
    ];
    for case in cases {
        let _ = std::fs::remove_file(&log);
        let output = workspace
            .cli()
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .arg("skill")
            .args(case)
            .output()
            .expect("skill");
        assert_eq!(
            output.status.code(),
            Some(4),
            "{case:?}: launcher exit must pass through; stderr: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), "{\"report\":\"node\"}\n", "{case:?}");
        let recorded = std::fs::read_to_string(&log).expect("launcher ran");
        let mut expected = vec!["marker=1".to_owned(), "arg=skill".to_owned()];
        expected.extend(case.iter().map(|arg| format!("arg={arg}")));
        assert_eq!(recorded.lines().collect::<Vec<_>>(), expected, "{case:?}");
    }
}

#[test]
fn delegation_marker_stops_a_second_forward() {
    let workspace = Workspace::new();
    let (bin, log) = stub_launcher(&workspace);
    let output = workspace
        .cli()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("OCTOCODE_SKILL_DELEGATED", "1")
        .args(["skill", "list", "--json"])
        .output()
        .expect("skill");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("native binary, not the npm CLI"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("npx -y octocode skill list --json"),
        "{}",
        stderr(&output)
    );
    assert!(!log.exists(), "launcher must not run under the marker");
}

#[test]
fn launcher_on_path_that_is_this_binary_fails_before_spawning() {
    let workspace = Workspace::new();
    let bin = workspace.home.join("self-bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_octocode"), bin.join("octocode"))
        .expect("symlink");
    let output = workspace
        .cli()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .args(["skill", "list"])
        .output()
        .expect("skill");
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("native binary, not the npm CLI"), "{text}");
    assert!(text.contains("resolves to this binary"), "{text}");
    assert_eq!(
        text.matches("octocode skill:").count(),
        1,
        "the guard must fire once, without a child: {text}"
    );
}

#[test]
fn missing_launcher_names_the_install_route() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["skill", "list", "--json"])
        .output()
        .expect("skill");
    assert_eq!(output.status.code(), Some(1));
    let text = stderr(&output);
    assert!(text.contains("npm i -g octocode"), "{text}");
    assert!(text.contains("npx -y octocode skill list --json"), "{text}");
}
