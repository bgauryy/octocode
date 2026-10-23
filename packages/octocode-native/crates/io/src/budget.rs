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
        check_until(self.effective_deadline(), &self.cancellation)
    }

    /// `future`'s output, unless the budget is cancelled or its deadline
    /// passes first.
    pub async fn wait<T>(&self, future: impl Future<Output = T>) -> Result<T, BudgetStop> {
        wait_until(self.effective_deadline(), &self.cancellation, future).await
    }
}

fn check_until(deadline: Instant, cancellation: &CancellationToken) -> Result<(), BudgetStop> {
    if cancellation.is_cancelled() {
        return Err(BudgetStop::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(BudgetStop::Deadline);
    }
    Ok(())
}

/// `future`'s output, unless `cancellation` fires or `deadline` passes first.
/// The one wait every budgeted await runs through.
pub async fn wait_until<T>(
    deadline: Instant,
    cancellation: &CancellationToken,
    future: impl Future<Output = T>,
) -> Result<T, BudgetStop> {
    check_until(deadline, cancellation)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    tokio::select! {
        _ = cancellation.cancelled() => Err(BudgetStop::Cancelled),
        value = tokio::time::timeout(remaining, future) => value.map_err(|_| BudgetStop::Deadline),
    }
}

/// Sleep `delay`, unless `cancellation` fires first. Fails fast with
/// [`BudgetStop::Deadline`] instead of sleeping when the sleep would end at
/// or past `deadline`: unlike [`wait_until`], it never starts a wait it
/// cannot finish.
pub async fn pause(
    delay: Duration,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<(), BudgetStop> {
    if cancellation.is_cancelled() {
        return Err(BudgetStop::Cancelled);
    }
    if Instant::now() + delay >= deadline {
        return Err(BudgetStop::Deadline);
    }
    if delay.is_zero() {
        return Ok(());
    }
    wait_until(deadline, cancellation, tokio::time::sleep(delay)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_pause_that_would_pass_the_deadline_fails_without_sleeping() {
        let token = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        let started = Instant::now();
        assert_eq!(
            pause(Duration::from_secs(60), deadline, &token).await,
            Err(BudgetStop::Deadline)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_pause_inside_the_deadline_sleeps_the_delay() {
        let token = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        let started = Instant::now();
        assert_eq!(
            pause(Duration::from_millis(40), deadline, &token).await,
            Ok(())
        );
        assert!(started.elapsed() >= Duration::from_millis(40));
        assert_eq!(pause(Duration::ZERO, deadline, &token).await, Ok(()));
    }

    #[tokio::test]
    async fn a_cancelled_token_stops_a_pause_before_and_during_the_sleep() {
        let token = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_secs(60);
        let sleeping = pause(Duration::from_secs(30), deadline, &token);
        let cancel = async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            token.cancel();
        };
        let (stopped, ()) = tokio::join!(sleeping, cancel);
        assert_eq!(stopped, Err(BudgetStop::Cancelled));
        assert_eq!(
            pause(Duration::ZERO, deadline, &token).await,
            Err(BudgetStop::Cancelled)
        );
    }

    #[tokio::test]
    async fn a_budget_wait_ends_at_its_deadline_or_cancellation() {
        let budget = RequestBudget::with_timeout(Duration::from_millis(30), 0);
        assert_eq!(
            budget.wait(std::future::pending::<()>()).await,
            Err(BudgetStop::Deadline)
        );
        assert_eq!(budget.check(), Err(BudgetStop::Deadline));
        let cancelled = RequestBudget::with_timeout(Duration::from_secs(30), 0);
        cancelled.cancellation.cancel();
        assert_eq!(
            cancelled.wait(async { 1 }).await,
            Err(BudgetStop::Cancelled)
        );
    }
}
