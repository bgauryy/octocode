//! The time the language-server client, its transport and the pool read and
//! wait on. Each client, connection and pool holds an [`LspClock`]: tokio's
//! monotonic clock in every build, or a [`Timer`] a test passes in to move
//! time itself.

use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

/// A wake-up at an instant of a [`Timer`]'s time.
pub(crate) type Wake = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Where a deadline goes when `now + duration` cannot be represented: thirty
/// years ahead, as tokio's own `sleep` does.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "not a wait or a timeout: the saturation point of a deadline that would overflow `Instant`, as tokio's `sleep` uses"
    )
)]
const FAR_FUTURE: Duration = Duration::from_secs(86_400 * 365 * 30);

/// A source of the current instant and of wake-ups at a later one.
pub(crate) trait Timer: Send + Sync {
    fn now(&self) -> Instant;
    /// Completes once [`Self::now`] reaches `deadline`, at once if it has.
    fn sleep_until(&self, deadline: Instant) -> Wake;
}

/// tokio's monotonic clock and timer wheel.
struct TokioTimer;

impl Timer for TokioTimer {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep_until(&self, deadline: Instant) -> Wake {
        Box::pin(tokio::time::sleep_until(deadline))
    }
}

/// A wait whose deadline came before the awaited work finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Elapsed;

/// The clock one pool, client or connection reads: tokio's unless built
/// from another [`Timer`]. Clones share the timer.
#[derive(Clone)]
pub(crate) struct LspClock(Arc<dyn Timer>);

impl Default for LspClock {
    fn default() -> Self {
        Self(Arc::new(TokioTimer))
    }
}

impl From<Arc<dyn Timer>> for LspClock {
    fn from(timer: Arc<dyn Timer>) -> Self {
        Self(timer)
    }
}

impl LspClock {
    pub(crate) fn now(&self) -> Instant {
        self.0.now()
    }

    /// Time since `earlier`; zero when `earlier` is not in the past.
    pub(crate) fn elapsed(&self, earlier: Instant) -> Duration {
        self.now().saturating_duration_since(earlier)
    }

    /// `duration` from now, or [`FAR_FUTURE`] from now when that sum cannot
    /// be represented.
    pub(crate) fn deadline_after(&self, duration: Duration) -> Instant {
        let now = self.now();
        now.checked_add(duration)
            .or_else(|| now.checked_add(FAR_FUTURE))
            .unwrap_or(now)
    }

    /// Completes at `deadline`, at once if it has passed.
    pub(crate) fn sleep_until(&self, deadline: Instant) -> Wake {
        self.0.sleep_until(deadline)
    }

    /// Completes `duration` from now.
    pub(crate) fn sleep(&self, duration: Duration) -> Wake {
        self.sleep_until(self.deadline_after(duration))
    }

    /// `work`'s output, or [`Elapsed`] when `deadline` comes first. `work`
    /// is polled before the deadline, so work that is ready wins a tie.
    pub(crate) fn timeout_at<F: IntoFuture>(
        &self,
        deadline: Instant,
        work: F,
    ) -> impl Future<Output = Result<F::Output, Elapsed>> + use<F> {
        let wake = self.sleep_until(deadline);
        let work = work.into_future();
        async move {
            let work = std::pin::pin!(work);
            tokio::select! {
                biased;
                output = work => Ok(output),
                () = wake => Err(Elapsed),
            }
        }
    }

    /// [`Self::timeout_at`] with the deadline `duration` from now, taken when
    /// this is called.
    pub(crate) fn timeout<F: IntoFuture>(
        &self,
        duration: Duration,
        work: F,
    ) -> impl Future<Output = Result<F::Output, Elapsed>> + use<F> {
        self.timeout_at(self.deadline_after(duration), work)
    }
}
