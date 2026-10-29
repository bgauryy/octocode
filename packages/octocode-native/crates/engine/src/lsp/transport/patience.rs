//! How long a request waits for a server that is slow but alive.
//!
//! A fixed wall budget cannot tell a hung server from a starved one: on a
//! busy machine (CI, parallel agents) a cold `rust-analyzer` or `tsgo` is
//! still indexing when the budget ends. After the base timeout the wait is
//! therefore extended in short steps, and only while the server shows
//! progress:
//!
//! * its process tree used a meaningful amount of CPU since the last check
//!   (the [`CpuClock`]), or
//! * it has an open `$/progress` work-done token (including rust-analyzer's
//!   "not quiescent" status).
//!
//! A server with neither is hung and times out at the base, exactly as before.
//! A ceiling bounds even a server that never stops making progress: the
//! caller's own deadline (less [`DEADLINE_RESERVE`]) when the request runs
//! inside [`with_caller_deadline`], so a starved server may use all the time
//! its caller still grants; otherwise (a background start or prewarm, which
//! no caller waits on) `CEILING_FACTOR` times the base. The attempts that
//! share one [`WaitBudget`] (a request and its retries) share that one
//! ceiling.

use super::progress::ProgressTracker;
use std::sync::Arc;
use tokio::time::{Duration, Instant, timeout_at};

/// Cumulative CPU time of the server's process tree; `None` when unreadable.
pub(crate) type CpuClock = Arc<dyn Fn() -> Option<Duration> + Send + Sync>;

/// The base timeout is split into this many check steps; each extension
/// grants one step.
const STEP_DIVISOR: u32 = 4;
/// The ceiling of a wait no caller bounds, as a multiple of the base timeout,
/// however long progress lasts.
pub(crate) const CEILING_FACTOR: u32 = 2;
/// CPU growth must reach `1 / CPU_PROGRESS_DIVISOR` (1%) of a step to count
/// as progress. A starved server gets very little: a cold rust-analyzer was
/// measured at about 5% of one core under heavy oversubscription, while a
/// hung or idle server shows none beyond timer ticks.
const CPU_PROGRESS_DIVISOR: u32 = 100;

/// What a request leaves its caller before the caller's deadline: the
/// `$/cancelRequest` write and shaping the failure row.
pub(crate) const DEADLINE_RESERVE: Duration = Duration::from_secs(1);

tokio::task_local! {
    static CALLER_DEADLINE: Instant;
}

/// Runs `work` for a caller that gives up at `deadline`: every request wait
/// it starts may extend, while its server progresses, up to that deadline
/// less [`DEADLINE_RESERVE`], and never past it.
pub async fn with_caller_deadline<F: Future>(deadline: Instant, work: F) -> F::Output {
    CALLER_DEADLINE.scope(deadline, work).await
}

/// What one request, retries included, may wait: `base` per attempt unless
/// the server shows progress, and no attempt past `ceiling` (see the module
/// docs). `Copy`, so the attempts of one request carry the same budget.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WaitBudget {
    pub(super) base: Duration,
    created: Instant,
    ceiling: Instant,
}

impl WaitBudget {
    /// A budget that starts now: `base_ms` per attempt, ending at the
    /// caller's deadline less [`DEADLINE_RESERVE`], or without a caller at
    /// `CEILING_FACTOR * base_ms` from now.
    pub(crate) fn new(base_ms: u32) -> Self {
        let base = Duration::from_millis(u64::from(base_ms));
        let created = Instant::now();
        let ceiling = CALLER_DEADLINE
            .try_with(|deadline| deadline.checked_sub(DEADLINE_RESERVE).unwrap_or(created))
            .unwrap_or(created + base * CEILING_FACTOR);
        Self {
            base,
            created,
            ceiling: ceiling.max(created),
        }
    }

    /// A budget that never extends: `base_ms` once, whatever the caller's
    /// deadline or the server's activity, for a request not worth outwaiting
    /// (a `shutdown` sent to a server being replaced).
    pub(crate) fn fixed(base_ms: u32) -> Self {
        let base = Duration::from_millis(u64::from(base_ms));
        let created = Instant::now();
        Self {
            base,
            created,
            ceiling: created + base,
        }
    }

    /// `true` once no further attempt can be granted any time.
    pub(crate) fn is_spent(&self) -> bool {
        Instant::now() >= self.ceiling
    }
}

impl From<u32> for WaitBudget {
    fn from(base_ms: u32) -> Self {
        Self::new(base_ms)
    }
}

/// The progress signals of one language server.
pub(super) struct ServerActivity {
    cpu_clock: Option<CpuClock>,
    progress: Arc<ProgressTracker>,
}

impl ServerActivity {
    pub(super) fn new(cpu_clock: Option<CpuClock>, progress: Arc<ProgressTracker>) -> Self {
        Self {
            cpu_clock,
            progress,
        }
    }

    /// Awaits `response` for `budget.base` from `started`, then in steps of a
    /// quarter of it while the server shows progress, never past the
    /// budget's ceiling. `Err` carries how long the budget has been waiting
    /// since it was created, all attempts included. Dropping the
    /// future cancels the wait at the next await, wherever in the extension
    /// it is.
    pub(super) async fn wait<F>(
        &self,
        mut response: F,
        started: Instant,
        budget: WaitBudget,
    ) -> Result<F::Output, Duration>
    where
        F: Future + Unpin,
    {
        let step = budget.base / STEP_DIVISOR;
        let ceiling = budget.ceiling;
        let mut deadline = (started + budget.base).min(ceiling);
        // The baseline sample is taken one step before the base deadline, so
        // the first decision needs no extra wait: a hung server is still cut
        // at exactly the base, and a request that ends sooner is never
        // sampled.
        let mut last_cpu =
            match timeout_at(deadline.checked_sub(step).unwrap_or(started), &mut response).await {
                Ok(output) => return Ok(output),
                Err(_) => self.cpu_time(),
            };
        loop {
            if let Ok(output) = timeout_at(deadline, &mut response).await {
                return Ok(output);
            }
            if deadline >= ceiling {
                return Err(deadline - budget.created);
            }
            let cpu = self.cpu_time();
            let progressed = cpu_progressed(last_cpu, cpu, step) || self.progress.has_active();
            if !progressed {
                return Err(deadline - budget.created);
            }
            last_cpu = cpu.or(last_cpu);
            deadline = (deadline + step).min(ceiling);
        }
    }

    fn cpu_time(&self) -> Option<Duration> {
        self.cpu_clock.as_ref().and_then(|clock| clock())
    }
}

/// `true` when CPU time grew by at least `step / CPU_PROGRESS_DIVISOR`
/// between two readable samples.
fn cpu_progressed(before: Option<Duration>, after: Option<Duration>, step: Duration) -> bool {
    match before.zip(after) {
        Some((before, after)) => after.saturating_sub(before) >= step / CPU_PROGRESS_DIVISOR,
        None => false,
    }
}
