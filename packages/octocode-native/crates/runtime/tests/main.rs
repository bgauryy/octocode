//! Single integration-test binary for octocode-native.
//!
//! Every `tests/*.rs` file is a module here instead of its own test target
//! (`autotests = false`). Separate targets each linked a full copy of the crate
//! and its dependencies per build variant: 19 binaries, ~2 GB per build, and
//! hundreds of GB of stale copies in target/. One binary links once and runs
//! all tests in one parallel harness. Run one file with
//! `cargo test -p octocode-native --test integration <module>::`.
#![allow(clippy::expect_used, reason = "integration-test assertions")]

mod support;

mod auth_discovery;
mod contract_field_effects;
mod runtime_batch_indices;
mod runtime_batch_response;
mod runtime_clasify;
mod runtime_clasify_routing;
mod runtime_d1_shapes;
mod runtime_empty_error_rows;
mod runtime_flow_edge_replay;
mod runtime_gh_structure;
mod runtime_github;
mod runtime_github_cache;
mod runtime_github_large_pr;
mod runtime_github_scope;
mod runtime_input_guard;
mod runtime_input_replay;
mod runtime_local;
mod runtime_page_replay;
mod runtime_read_span;
mod runtime_request_credential;
mod runtime_stream_page_walk;
mod runtime_surface_arms;
mod runtime_tool_flow_handoffs;
mod tool_cache_contracts;

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
