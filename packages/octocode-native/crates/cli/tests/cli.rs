use crate::support;

use std::process::Output;
use support::Workspace;
mod credentials;
mod help;
mod install;
mod output;
mod schema;
mod tool_input;

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap_or("")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap_or("")
}

fn exit_code(output: &Output) -> Option<i32> {
    output.status.code()
}

/// A JSON-mode error is the error itself: no `kind`/`version`/`tool` frame.
fn assert_bare_error(error: &serde_json::Value) {
    for frame in ["kind", "version", "tool"] {
        assert!(error.get(frame).is_none(), "{frame}: {error}");
    }
}

/// Opens every availability gate so root help lists every contract tool.
fn all_tools_enabled(command: &mut std::process::Command) -> &mut std::process::Command {
    command
        .env("OCTOCODE_CLASSIFICATION_API", "fixture-key")
        .env("OCTOCODE_BETA", "true")
        // Clone outputs (ghCloneRepo) need persistent storage.
        .env("OCTOCODE_STORAGE_MODE", "persistent")
}
