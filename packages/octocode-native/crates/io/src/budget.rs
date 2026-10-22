//! The one deadline-and-cancellation budget every provider request runs under
//! (GitHub, artifact registries, classification).
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

/// Why a [`RequestBudget`] stopped a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetStop {
    Cancelled,
    Deadline,
}

/// Provider-neutral resource limits. Credentials remain in each provider's
/// request context and cannot cross into another ecosystem through this type.
/// Clones share the time credited to rate-limit waits.
#[derive(Clone, Debug)]
pub struct RequestBudget {
    /// When the request time (the runtime's network timeout) runs out.
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    pub max_body_bytes: usize,
    /// The latest a rate-limit wait may end: the tool call's own deadline.
    /// Time spent waiting on a rate limit moves the deadline back by as much
    /// (up to here), so a wait never spends the request time. Equal to
    /// `deadline`: waits get no extra time.
    pub wait_until: Instant,
    /// Milliseconds of rate-limit waiting credited so far.
    waited_ms: Arc<AtomicU64>,
}

impl RequestBudget {
    pub fn new(deadline: Instant, cancellation: CancellationToken, max_body_bytes: usize) -> Self {
        Self {
            deadline,
            cancellation,
            max_body_bytes,
            wait_until: deadline,
            waited_ms: Arc::default(),
        }
    }

    pub fn with_timeout(timeout: Duration, max_body_bytes: usize) -> Self {
        Self::new(
            Instant::now() + timeout,
            CancellationToken::new(),
            max_body_bytes,
        )
    }

    /// The request deadline moved back by every credited wait, never past
    /// `wait_until`.
    pub fn effective_deadline(&self) -> Instant {
        let waited = Duration::from_millis(self.waited_ms.load(Ordering::Relaxed));
        (self.deadline + waited).min(self.wait_until.max(self.deadline))
    }

    /// The latest an admission wait starting now may end: the request time
    /// left or `cap`, whichever ends later, within `wait_until`.
    pub fn admission_deadline(&self, cap: Duration) -> Instant {
        self.effective_deadline()
            .max(self.wait_until.min(Instant::now() + cap))
    }

    /// Credit `waited` spent on a rate limit, not on the request.
    pub fn credit_wait(&self, waited: Duration) {
        let millis = u64::try_from(waited.as_millis()).unwrap_or(u64::MAX);
        self.waited_ms.fetch_add(millis, Ordering::Relaxed);
    }

    /// Cancellation first, then the deadline.
    pub fn check(&self) -> Result<(), BudgetStop> {
        if self.cancellation.is_cancelled() {
            return Err(BudgetStop::Cancelled);
        }
        if Instant::now() >= self.effective_deadline() {
            return Err(BudgetStop::Deadline);
        }
        Ok(())
    }

    /// `future`'s output, unless the budget is cancelled or its deadline
    /// passes first.
    pub async fn wait<T>(&self, future: impl Future<Output = T>) -> Result<T, BudgetStop> {
        self.check()?;
        let remaining = self
            .effective_deadline()
            .saturating_duration_since(Instant::now());
        tokio::select! {
            _ = self.cancellation.cancelled() => Err(BudgetStop::Cancelled),
            value = tokio::time::timeout(remaining, future) => value.map_err(|_| BudgetStop::Deadline),
        }
    }
}
