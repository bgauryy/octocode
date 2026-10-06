//! Single integration-test binary for octocode-cli.
//!
//! Every `tests/*.rs` file is a module here instead of its own test target
//! (`autotests = false`). Separate targets each linked a full copy of the crate
//! and its dependencies per build variant: 8 binaries, ~2 GB per build, and
//! hundreds of GB of stale copies in target/. One binary links once and runs
//! all tests in one parallel harness. Run one file with
//! `cargo test -p octocode-cli --test integration <module>::`.
#![allow(clippy::expect_used, reason = "integration-test assertions")]

mod support;

mod auth_discovery;
mod cli;
mod cli_batch_response;
mod cli_exit_codes;
mod cli_scheme;
mod cli_skill;
mod cli_tool_contract;
mod config_edit;

/// `autotests = false` means a `tests/*.rs` file missing from the list above
/// would silently never run.
#[test]
fn every_test_file_is_a_module() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let main = std::fs::read_to_string(dir.join("main.rs")).expect("read tests/main.rs");
    for entry in std::fs::read_dir(&dir).expect("read tests/") {
        let path = entry.expect("tests/ entry").path();
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if stem != "main" && path.extension().is_some_and(|ext| ext == "rs") {
            assert!(
                main.contains(&format!("mod {stem};")),
                "tests/{stem}.rs is not declared in tests/main.rs"
            );
        }
    }
}
