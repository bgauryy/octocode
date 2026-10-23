//! Standalone skill commands run without any launchers on PATH.

use crate::support::Workspace;
use serde_json::Value;
use std::{fs, path::Path, process::Output};
mod install;
mod scopes;

const SKILL: &str = "octocode-workflows";
const ARCHITECT: &str = "octocode-architect";
fn run(workspace: &Workspace, args: &[&str]) -> Output {
    workspace
        .cli()
        .env("PATH", workspace.home.join("no-executables"))
        .env("USERPROFILE", &workspace.home)
        .arg("skill")
        .args(args)
        .arg("--json")
        .output()
        .expect("native skill")
}
fn report(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON report")
}
fn local_skill(directory: &Path, name: &str, version: &str) {
    fs::create_dir_all(directory.join("references")).unwrap();
    fs::write(
        directory.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Local fixture\n---\n{version}\n"),
    )
    .unwrap();
    fs::write(directory.join("references/guide.md"), version).unwrap();
}
