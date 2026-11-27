//! The one deadline-and-cancellation budget every provider request runs under
//! (GitHub, artifact registries, classification).
use crate::cancellation::Cancellation;
use std::{
    future::Future,
    time::{Duration, Instant},
};

/// The most one provider response body may hold.
pub const PROVIDER_BODY_BYTES: usize = 16 * 1024 * 1024;

/// Why a [`RequestBudget`] stopped a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetStop {
    Cancelled,
    Deadline,
}

/// Provider-neutral resource limits. Credentials remain in each provider's
/// request context and cannot cross into another ecosystem through this type.
#[derive(Clone, Debug)]
pub struct RequestBudget {
    /// When the request time (the runtime's network timeout) runs out. Every
    /// wait, rate-limit waits included, ends by it.
    pub deadline: Instant,
    pub cancellation: Cancellation,
    pub max_body_bytes: usize,
    /// The longest one network read may wait for its next bytes
    /// ([`Self::wait_stalled`]); `None` for none beyond the deadline.
    stall: Option<Duration>,
}

impl RequestBudget {
    pub fn new(deadline: Instant, cancellation: Cancellation, max_body_bytes: usize) -> Self {
        Self {
            deadline,
            cancellation,
            max_body_bytes,
            stall: None,
        }
    }

    /// This budget with a stall bound: one network read (connect and headers,
    /// or the next body chunk) that gets no bytes for `stall` ends the
    /// request. A transfer that keeps flowing is bounded only by the
    /// deadline and cancellation.
    #[must_use]
    pub fn with_stall(mut self, stall: Duration) -> Self {
        self.stall = Some(stall);
        self
    }

    /// The stall bound [`Self::wait_stalled`] applies, if any.
    #[must_use]
    pub fn stall(&self) -> Option<Duration> {
        self.stall
    }

    pub fn with_timeout(timeout: Duration, max_body_bytes: usize) -> Self {
        Self::new(
            Instant::now() + timeout,
            Cancellation::new(),
            max_body_bytes,
        )
    }

    /// Cancellation first, then the deadline.
    pub fn check(&self) -> Result<(), BudgetStop> {
        check_until(self.deadline, &self.cancellation)
    }

    /// [`Self::wait`] for one network read: `future` must also finish within
    /// the stall bound, which every read starts anew. A stall reports
    /// [`BudgetStop::Deadline`], as any request timeout does.
    pub async fn wait_stalled<T>(&self, future: impl Future<Output = T>) -> Result<T, BudgetStop> {
        let now = Instant::now();
        let end = self.deadline;
        let end = self.stall.map_or(end, |stall| end.min(now + stall));
        wait_until(end, &self.cancellation, future).await
    }

    /// `future`'s output, unless the budget is cancelled or its deadline
    /// passes first. A plain function returning [`wait_until`]'s future: an
    /// `async` wrapper would hold `future` twice, once as its argument and
    /// once inside the wait it awaits.
    pub fn wait<T>(
        &self,
        future: impl Future<Output = T>,
    ) -> impl Future<Output = Result<T, BudgetStop>> {
        wait_until(self.deadline, &self.cancellation, future)
    }
}

fn check_until(deadline: Instant, cancellation: &Cancellation) -> Result<(), BudgetStop> {
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
    cancellation: &Cancellation,
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
    cancellation: &Cancellation,
) -> Result<(), BudgetStop> {
    start_pause(delay, deadline, cancellation)?;
    if delay.is_zero() {
        return Ok(());
    }
    wait_until(deadline, cancellation, tokio::time::sleep(delay)).await
}

fn start_pause(
    delay: Duration,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<(), BudgetStop> {
    if cancellation.is_cancelled() {
        return Err(BudgetStop::Cancelled);
    }
    if Instant::now() + delay >= deadline {
        return Err(BudgetStop::Deadline);
    }
    Ok(())
}

/// How often a long wait says it is still going: a heartbeat for hosts that
/// reset their own timeout on progress. It does not limit the wait; only the
/// deadline does, and it is the same whenever the wait began.
pub const HEARTBEAT: Duration = Duration::from_secs(10);

/// The shortest wait worth telling anyone about: waits report whole
/// seconds, and a shorter one is spacing between requests, not a pause.
pub const REPORTED_FROM: Duration = Duration::from_secs(1);

/// [`pause`] that tells the call's host about the wait ([`notice::progress_sink`](crate::notice::progress_sink)):
/// `line(remaining)` when it starts and again every [`HEARTBEAT`] while it
/// lasts. A wait the deadline refuses is refused before it is announced, and
/// a call nobody listens to, or a wait under [`REPORTED_FROM`], pauses
/// silently.
pub async fn pause_reporting(
    delay: Duration,
    deadline: Instant,
    cancellation: &Cancellation,
    line: impl Fn(Duration) -> String,
) -> Result<(), BudgetStop> {
    pause_beating(delay, HEARTBEAT, deadline, cancellation, line).await
}

async fn pause_beating(
    delay: Duration,
    beat: Duration,
    deadline: Instant,
    cancellation: &Cancellation,
    line: impl Fn(Duration) -> String,
) -> Result<(), BudgetStop> {
    let sink = if delay >= REPORTED_FROM {
        crate::notice::progress_sink()
    } else {
        None
    };
    let Some(sink) = sink else {
        return pause(delay, deadline, cancellation).await;
    };
    start_pause(delay, deadline, cancellation)?;
    let mut remaining = delay;
    while !remaining.is_zero() {
        sink(&line(remaining));
        let step = remaining.min(beat);
        wait_until(deadline, cancellation, tokio::time::sleep(step)).await?;
        remaining -= step;
    }
    Ok(())
}

/// `work`, with a line to the call's host ([`crate::notice::progress_sink`])
/// once it has run [`REPORTED_FROM`] and every [`HEARTBEAT`] after, each
/// `line(elapsed)`: how a cold start or an indexing wait that no deadline
/// refuses says it is still going. Work faster than [`REPORTED_FROM`], or a
/// call nobody listens to, is silent; the wait itself is not limited.
pub async fn report_while<F: Future>(work: F, line: impl Fn(Duration) -> String) -> F::Output {
    report_beating(work, REPORTED_FROM, HEARTBEAT, line).await
}

async fn report_beating<F: Future>(
    work: F,
    from: Duration,
    beat: Duration,
    line: impl Fn(Duration) -> String,
) -> F::Output {
    let Some(sink) = crate::notice::progress_sink() else {
        return work.await;
    };
    let mut work = std::pin::pin!(work);
    let started = Instant::now();
    let mut next = from;
    loop {
        match tokio::time::timeout(next, work.as_mut()).await {
            Ok(output) => return output,
            Err(_) => {
                sink(&line(started.elapsed()));
                next = beat;
            }
        }
    }
}

#[cfg(test)]
mod tests;
