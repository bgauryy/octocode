//! Limiter refusals of a git transfer: provider failures versus the caller's own stop.
use super::admission::admission_error;
use super::*;
use crate::tools::gh_clone_repo::CloneFailure;
use octocode_github::{ProviderError, ProviderErrorKind, RateLimit};

fn limited(kind: ProviderErrorKind) -> ProviderError {
    let mut error = ProviderError::new(kind, "GitHub secondary rate limit; request not sent.");
    error.rate_limit = Some(Box::new(RateLimit {
        remaining: None,
        reset_epoch_seconds: None,
        retry_after_seconds: Some(42),
        resource: None,
    }));
    error.retryable = true;
    error
}

/// A limiter cooldown is the limiter's provider failure, not clone text:
/// the row keeps its rate-limit block, retryability and retry delay.
#[test]
fn a_limiter_rate_limit_is_the_rows_provider_failure() {
    match CloneFailure::from(admission_error(limited(ProviderErrorKind::RateLimited))) {
        CloneFailure::Provider(error) => {
            assert_eq!(error.kind, ProviderErrorKind::RateLimited);
            assert!(error.retryable);
            assert_eq!(
                error.rate_limit.and_then(|rate| rate.retry_after_seconds),
                Some(42)
            );
        }
        CloneFailure::Clone(error) => panic!("rate limit flattened to text: {error:?}"),
    }
}

/// An open circuit names its real cause (an outage), not a rate limit.
#[test]
fn an_open_circuit_keeps_its_cause() {
    match CloneFailure::from(admission_error(limited(ProviderErrorKind::Transport))) {
        CloneFailure::Provider(error) => assert_eq!(error.kind, ProviderErrorKind::Transport),
        CloneFailure::Clone(error) => panic!("circuit flattened to text: {error:?}"),
    }
}

/// The caller's own cancellation or deadline stays the clone's error.
#[test]
fn the_callers_own_stop_stays_a_clone_error() {
    for (kind, code) in [
        (ProviderErrorKind::Cancelled, ErrorCode::Cancelled),
        (ProviderErrorKind::Timeout, ErrorCode::Timeout),
    ] {
        match CloneFailure::from(admission_error(ProviderError::new(kind, "stopped"))) {
            CloneFailure::Clone(error) => assert_eq!(error.code, code),
            CloneFailure::Provider(error) => panic!("caller stop became provider: {error:?}"),
        }
    }
}
