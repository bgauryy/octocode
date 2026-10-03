//! Runtime fixtures plus Cargo-owned CLI executable discovery.
#[path = "../../../runtime/tests/support/mod.rs"]
mod runtime_fixture;
pub use runtime_fixture::*;
use std::process::Command;

impl Workspace {
    pub fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_octocode"));
        command
            .current_dir(&self.workspace)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("OCTOCODE_HOME", &self.home)
            .env("WORKSPACE_ROOT", &self.workspace)
            .env("ALLOWED_PATHS", &self.workspace)
            .env("ENABLE_LOCAL", "true")
            .env("ENABLE_CLONE", "false")
            .env("NO_COLOR", "1")
            .env("OCTOCODE_ENABLE_STATS", "false");
        command
    }
}
