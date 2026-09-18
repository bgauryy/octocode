use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Provider-neutral resource limits. Credentials remain in each provider's
/// request context and cannot cross into another ecosystem through this type.
#[derive(Clone, Debug)]
pub struct RequestBudget {
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    pub max_body_bytes: usize,
}

impl RequestBudget {
    pub fn with_timeout(timeout: Duration, max_body_bytes: usize) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            cancellation: CancellationToken::new(),
            max_body_bytes,
        }
    }
}
