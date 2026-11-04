//! The one deadline-and-cancellation budget every provider request runs under
//! (GitHub, artifact registries, classification).
use crate::cancellation::Cancellation;
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
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
/// Clones share the time credited to rate-limit waits.
#[derive(Clone, Debug)]
pub struct RequestBudget {
    /// When the request time (the runtime's network timeout) runs out.
    pub deadline: Instant,
    pub cancellation: Cancellation,
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
    pub fn new(deadline: Instant, cancellation: Cancellation, max_body_bytes: usize) -> Self {
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
            Cancellation::new(),
            max_body_bytes,
        )
    }

    /// The request deadline moved back by every credited wait, never past
    /// `wait_until`.
    pub fn effective_deadline(&self) -> Instant {
        let waited = Duration::from_millis(self.waited_ms.load(Ordering::Relaxed));
        (self.deadline + waited).min(self.wait_until.max(self.deadline))
    }

    /// The latest an admission wait may end: the call's own deadline
    /// (`wait_until`), or the request time left when that is later. A
    /// rate-limit wait is refused only when it ends past this.
    pub fn admission_deadline(&self) -> Instant {
        self.effective_deadline().max(self.wait_until)
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

/// [`pause`] that tells the call's host about the wait ([`notice::progress_sink`]):
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_pause_that_would_pass_the_deadline_fails_without_sleeping() {
        let token = Cancellation::new();
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
        let token = Cancellation::new();
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
        let token = Cancellation::new();
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

    /// A reporting wait says its remaining time when it starts and at each
    /// beat, then ends on time; it says nothing to a call nobody listens to,
    /// and nothing for a wait the deadline refuses.
    #[tokio::test]
    async fn a_reporting_pause_beats_while_it_waits() {
        let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&heard);
        let scope = crate::notice::Scope::with_progress(move |line| {
            sink.lock().expect("lines").push(line.to_owned());
        });
        let token = Cancellation::new();
        let deadline = Instant::now() + Duration::from_secs(60);
        let delay = REPORTED_FROM * 2 + Duration::from_millis(100);
        let beat = REPORTED_FROM;
        let line = |remaining: Duration| format!("{}ms", remaining.as_millis());
        let started = Instant::now();
        scope
            .run(pause_beating(delay, beat, deadline, &token, line))
            .await
            .expect("waited");
        assert!(started.elapsed() >= delay);
        assert_eq!(*heard.lock().expect("lines"), ["2100ms", "1100ms", "100ms"]);

        heard.lock().expect("lines").clear();
        let refused = scope.run(pause_beating(delay, beat, started, &token, line));
        assert_eq!(refused.await, Err(BudgetStop::Deadline));
        scope
            .run(pause_beating(
                REPORTED_FROM / 10,
                beat,
                deadline,
                &token,
                line,
            ))
            .await
            .expect("short");
        pause_beating(REPORTED_FROM, beat, deadline, &token, line)
            .await
            .expect("unheard");
        assert!(heard.lock().expect("lines").is_empty());
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

    /// Work that outlasts the first wait says so then and at each beat,
    /// returns its own output, and says nothing when it is quick.
    #[tokio::test]
    async fn slow_work_reports_while_it_runs() {
        let heard = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let sink = std::sync::Arc::clone(&heard);
        let scope = crate::notice::Scope::with_progress(move |_| {
            *sink.lock().expect("count") += 1;
        });
        let from = Duration::from_millis(30);
        let beat = Duration::from_millis(20);
        let slow = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            7
        };
        let out = scope
            .run(report_beating(slow, from, beat, |_| String::new()))
            .await;
        assert_eq!(out, 7);
        assert!(*heard.lock().expect("count") >= 3);
        *heard.lock().expect("count") = 0;
        let quick = scope
            .run(report_beating(async { 1 }, from, beat, |_| String::new()))
            .await;
        assert_eq!(quick, 1);
        assert_eq!(*heard.lock().expect("count"), 0);
    }
}
