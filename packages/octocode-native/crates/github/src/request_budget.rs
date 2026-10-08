//! The one deadline-and-cancellation budget every provider request runs under.
use std::{
    future::Future,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

use super::{ProviderError, ProviderErrorKind};

/// Why a [`RequestBudget`] stopped a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetStop {
    Cancelled,
    Deadline,
}

impl From<BudgetStop> for ProviderError {
    fn from(stop: BudgetStop) -> Self {
        match stop {
            BudgetStop::Cancelled => {
                ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")
            }
            BudgetStop::Deadline => ProviderError::new(
                ProviderErrorKind::Timeout,
                "GitHub request deadline exceeded",
            ),
        }
    }
}

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

    /// Cancellation first, then the deadline.
    pub fn check(&self) -> Result<(), BudgetStop> {
        if self.cancellation.is_cancelled() {
            return Err(BudgetStop::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(BudgetStop::Deadline);
        }
        Ok(())
    }

    /// `future`'s output, unless the budget is cancelled or its deadline
    /// passes first.
    pub async fn wait<T>(&self, future: impl Future<Output = T>) -> Result<T, BudgetStop> {
        self.check()?;
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        tokio::select! {
            _ = self.cancellation.cancelled() => Err(BudgetStop::Cancelled),
            value = tokio::time::timeout(remaining, future) => value.map_err(|_| BudgetStop::Deadline),
        }
    }
}
