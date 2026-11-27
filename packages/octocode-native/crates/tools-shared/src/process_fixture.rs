//! This test binary's re-executed entry point for
//! `octocode_runtime_base::process_fixture`, which re-runs its own test binary
//! by this test's name (`process_fixture::fixture`).

/// Without a fixture role it is an ordinary passing test.
#[test]
fn fixture() {
    octocode_runtime_base::process_fixture::entry();
}
