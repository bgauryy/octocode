//! Test evaluators' credentials and models, each unique in the process.
//! The classification gate keeps a refused credential, a spent quota and an
//! open circuit per endpoint and credential, and the windows an endpoint
//! taught are kept per endpoint and model, for the whole process; a mock
//! server's port is reused by a later test. Two tests that minted the same
//! key or model from counters of their own could observe each other's
//! failures (a test's 401 denying another test's requests before any is
//! sent). One counter for every test evaluator makes that impossible.
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// `<prefix>-<n>`: never returned twice in this process.
pub fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
