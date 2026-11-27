//! Fixtures the unit tests share: an evaluator against a mock provider and a
//! call context.
use crate::judgment::Evaluator;
use octocode_runtime_base::execution::ExecutionContext;
use std::time::{Duration, Instant};

/// The same, as a model of its own: what an endpoint's windows have taught
/// is kept per endpoint and model, and a mock server's port can be reused.
/// Each evaluator's credential is its own, unique in the process
/// ([`crate::judgment::test_identity`]): the gate remembers a refused
/// credential, a quota and an open circuit per endpoint and credential
/// digest, and a counter of this module alone minted keys another module's
/// evaluators also minted.
pub(super) fn evaluator_of(key: &str, base_url: String, model: &str) -> Evaluator {
    let key = crate::judgment::test_identity::unique(key);
    Evaluator::new(
        Some(secrecy::SecretString::from(key)),
        base_url,
        model.into(),
        &octocode_runtime_base::providers::classification::jev::JEV,
        Some(Duration::from_secs(10)),
        0,
        4,
    )
}

pub(super) fn context(timeout: Duration) -> ExecutionContext {
    ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: Some(Instant::now() + timeout),
        response_window: None,
        github_credential: None,
    }
}
