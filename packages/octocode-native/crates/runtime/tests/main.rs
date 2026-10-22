//! Single integration-test binary for octocode-native.
//!
//! Every `tests/*.rs` file is a module here instead of its own test target
//! (`autotests = false`). Separate targets each linked a full copy of the crate
//! and its dependencies per build variant: 19 binaries, ~2 GB per build, and
//! hundreds of GB of stale copies in target/. One binary links once and runs
//! all tests in one parallel harness. Run one file with
//! `cargo dev-test -- <module>::` (`.cargo/config.toml`): the `test:rust`
//! selection, so it reuses that build instead of compiling another copy.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration tests assert by unwrapping, expecting and panicking"
)]

mod support;

mod auth_discovery;
mod contract_field_effects;
mod lock_poison_policy;
mod runtime_artifact_search;
mod runtime_batch_indices;
mod runtime_batch_response;
mod runtime_clasify;
mod runtime_clasify_media;
mod runtime_clasify_sections;
mod runtime_clasify_url;
mod runtime_d1_shapes;
mod runtime_empty_error_rows;
mod runtime_flow_edge_replay;
mod runtime_generic_walk;
mod runtime_gh_default_branch;
mod runtime_gh_structure;
mod runtime_github;
mod runtime_github_cache;
mod runtime_github_compare;
mod runtime_github_flows;
mod runtime_github_history;
mod runtime_github_large_pr;
mod runtime_github_scope;
mod runtime_github_throttle;
mod runtime_input_guard;
mod runtime_input_replay;
mod runtime_local;
mod runtime_page_replay;
mod runtime_path_policy_names;
mod runtime_read_span;
mod runtime_repair_leads;
mod runtime_request_credential;
mod runtime_stream_page_walk;
mod runtime_surface_arms;
mod runtime_tool_flow_handoffs;
mod tool_cache_contracts;

/// `autotests = false` means a `tests/*.rs` file missing from the list above
/// would silently never run.
#[test]
fn every_test_file_is_a_module() {
    support::assert_every_test_file_is_a_module(env!("CARGO_MANIFEST_DIR"));
}
