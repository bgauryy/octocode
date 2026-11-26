//! Runtime fixtures plus Cargo-owned CLI executable discovery.
#[expect(
    dead_code,
    unused_imports,
    reason = "the CLI tests use a subset of the runtime integration fixtures"
)]
#[path = "../../../runtime/tests/support/mod.rs"]
mod runtime_fixture;
pub(crate) use runtime_fixture::*;
use std::path::PathBuf;
use std::process::Command;

mod persistent_cli;
pub(crate) use persistent_cli::PersistentCli;

/// Cargo and nextest supply the executable at run time so archived tests relocate.
/// Direct test-binary runs fall back to Cargo's compiled path.
/// See <https://nexte.st/docs/ci-features/archiving/#making-tests-relocatable>.
fn cli_executable() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_octocode")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_octocode")))
}

impl Workspace {
    /// The CLI in this workspace. Tool calls run in process
    /// (`storage.mode = memory`), so no test leaves a warm server behind;
    /// Persistent tests use `persistent_cli` to own and stop their servers.
    pub(crate) fn cli(&self) -> Command {
        let mut command = Command::new(cli_executable());
        command
            .current_dir(&self.workspace)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("OCTOCODE_HOME", &self.home)
            .env("WORKSPACE_ROOT", &self.workspace)
            .env("ALLOWED_PATHS", &self.workspace)
            .env("OCTOCODE_ENABLE_LOCAL", "true")
            .env("NO_COLOR", "1")
            .env("OCTOCODE_STORAGE_MODE", "memory")
            .env("GITHUB_API_URL", UNMOCKED_GITHUB_API_URL);
        // Coverage runs (`coverage:rust`) profile the CLI through this variable.
        // Without it the instrumented binary writes `default_*.profraw` into the
        // workspace, where later tool calls find it, and its coverage is lost.
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command
    }

    /// Calls sharing durable state and a private warm-server namespace.
    pub(crate) fn persistent_cli(&self) -> PersistentCli<'_> {
        PersistentCli::new(self)
    }
}

/// The research skill's reference files, the sources of the served references.
pub(crate) fn research_references() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../skills/octocode-research/references")
}

/// The tool-flow guide as served, from the skill files: `tool-flow.md`, then
/// the sibling parts its opening line links, in that order, each without its
/// opening line and with a blank line between parts.
pub(crate) fn tool_flow_guide() -> String {
    let read = |file: &str| {
        std::fs::read_to_string(research_references().join(file))
            .unwrap_or_else(|error| panic!("{file}: {error}"))
    };
    let first = read("tool-flow.md");
    let opening = first.lines().next().expect("opening line");
    let siblings = opening
        .split("](")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .map(read);
    let parts: Vec<String> = std::iter::once(first.clone()).chain(siblings).collect();
    assert!(parts.len() > 1, "tool-flow.md links no sibling part");
    parts
        .iter()
        .map(|part| {
            part.split_once("\n\n")
                .expect("opening line, then sections")
                .1
        })
        .collect::<Vec<_>>()
        .join("\n")
}
