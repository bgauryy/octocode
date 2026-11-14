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
//! * it sent a new `$/progress` work-done notice (a `begin`, `report` or
//!   `end`, or a rust-analyzer status change) since the last check. A token
//!   left open with nothing new said is no progress; or
//! * it sent any other message since the last check (a log line, a status,
//!   diagnostics, another request's answer): a server working through an
//!   external build tool (Metals over its build server) may use little CPU of
//!   its own while it talks.
//!
//! A server with none of these is idle on the request and is cut at the
//! base: the base is the window in which a live server shows any sign of
//! work, not a bound on work, which extends for as long as it lasts. The
//! failure says which ended the wait ([`Unanswered`]).
//! A server that spins its CPU without answering still counts as progressing;
//! without a caller deadline only a cancel ends that wait.
//! A ceiling bounds even a server that never stops making progress: the
//! caller's own deadline (less [`DEADLINE_RESERVE`]) when the request runs
//! inside [`with_caller_deadline`], so a starved server may use all the time
//! its caller still grants; none for a caller with no deadline, whose
//! cancel ends the wait; otherwise (a background start or prewarm, which no
//! caller waits on) `CEILING_FACTOR` times the base. The attempts that
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
    static CALLER_DEADLINE: Option<Instant>;
}

/// Runs `work` for a caller that gives up at `deadline`: every request wait
/// it starts may extend, while its server progresses, up to that deadline
/// less [`DEADLINE_RESERVE`], and never past it. With no deadline a wait
/// extends for as long as the server progresses; dropping `work` (the
/// caller's cancel) ends it.
pub async fn with_caller_deadline<F: Future>(deadline: Option<Instant>, work: F) -> F::Output {
    CALLER_DEADLINE.scope(deadline, work).await
}

/// What one request, retries included, may wait: `base` per attempt unless
/// the server shows progress, and no attempt past `ceiling` (see the module
/// docs). `Copy`, so the attempts of one request carry the same budget.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WaitBudget {
    pub(super) base: Duration,
    created: Instant,
    /// `None`: a caller with no deadline; progress alone decides.
    ceiling: Option<Instant>,
}

impl WaitBudget {
    /// A budget that starts now: `base_ms` per attempt, ending at the
    /// caller's deadline less [`DEADLINE_RESERVE`], never for a caller with
    /// no deadline, or without a caller at `CEILING_FACTOR * base_ms` from
    /// now.
    pub(crate) fn new(base_ms: u32) -> Self {
        let base = Duration::from_millis(u64::from(base_ms));
        let created = Instant::now();
        let ceiling = match CALLER_DEADLINE.try_with(|deadline| *deadline) {
            Ok(Some(deadline)) => Some(
                deadline
                    .checked_sub(DEADLINE_RESERVE)
                    .unwrap_or(created)
                    .max(created),
            ),
            Ok(None) => None,
            Err(_) => Some(created + base * CEILING_FACTOR),
        };
        Self {
            base,
            created,
            ceiling,
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
            ceiling: Some(created + base),
        }
    }

    /// `at`, or the ceiling when that comes first.
    pub(super) fn capped(&self, at: Instant) -> Instant {
        self.ceiling.map_or(at, |ceiling| at.min(ceiling))
    }

    /// When every attempt ends however long the server works: the caller's
    /// deadline less [`DEADLINE_RESERVE`], `None` for a caller with no
    /// deadline, else `CEILING_FACTOR` times the base from its start.
    pub(crate) fn ceiling(&self) -> Option<Instant> {
        self.ceiling
    }

    /// `true` once no further attempt can be granted any time.
    pub(crate) fn is_spent(&self) -> bool {
        self.ceiling
            .is_some_and(|ceiling| Instant::now() >= ceiling)
    }
}

impl From<u32> for WaitBudget {
    fn from(base_ms: u32) -> Self {
        Self::new(base_ms)
    }
}

/// Why a request wait ended without its answer.
#[derive(Debug, Clone, Copy)]
pub(super) struct Unanswered {
    /// How long the budget has waited since it was created, all attempts
    /// included.
    pub(super) waited: Duration,
    /// The span over which the server showed no activity at all, when that
    /// ended the wait; `None` when the ceiling did while it still worked.
    pub(super) idle: Option<Duration>,
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
    /// budget's ceiling. `Err` says how long the budget waited and whether
    /// an idle server or the ceiling ended it. Dropping the
    /// future cancels the wait at the next await, wherever in the extension
    /// it is.
    pub(super) async fn wait<F>(
        &self,
        mut response: F,
        started: Instant,
        budget: WaitBudget,
    ) -> Result<F::Output, Unanswered>
    where
        F: Future + Unpin,
    {
        let step = budget.base / STEP_DIVISOR;
        let mut deadline = budget.capped(started + budget.base);
        // The baseline sample is taken one step before the base deadline, so
        // the first decision needs no extra wait: a hung server is still cut
        // at exactly the base, and a request that ends sooner is never
        // sampled.
        let mut last_cpu =
            match timeout_at(deadline.checked_sub(step).unwrap_or(started), &mut response).await {
                Ok(output) => return Ok(output),
                Err(_) => self.cpu_time(),
            };
        // Progress is what changed over the step: new CPU time, a new
        // work-done notice or any other message. A token left open with
        // nothing new said (leaked) is not progress, so it cannot hold a
        // request without a ceiling.
        let mut last_signals = self.signals();
        let mut sampled = deadline.checked_sub(step).unwrap_or(started);
        loop {
            if let Ok(output) = timeout_at(deadline, &mut response).await {
                return Ok(output);
            }
            if budget.ceiling.is_some_and(|ceiling| deadline >= ceiling) {
                return Err(Unanswered {
                    waited: deadline - budget.created,
                    idle: None,
                });
            }
            let cpu = self.cpu_time();
            let signals = self.signals();
            let progressed = cpu_progressed(last_cpu, cpu, step) || signals != last_signals;
            if !progressed {
                return Err(Unanswered {
                    waited: deadline - budget.created,
                    idle: Some(deadline - sampled),
                });
            }
            last_cpu = cpu.or(last_cpu);
            last_signals = signals;
            sampled = deadline;
            deadline = budget.capped(deadline + step);
        }
    }

    /// The server's work-done notices and messages so far: either changing
    /// is a sign of work.
    fn signals(&self) -> (u64, u64) {
        (self.progress.reports(), self.progress.messages())
    }

    pub(super) fn cpu_time(&self) -> Option<Duration> {
        self.cpu_clock.as_ref().and_then(|clock| clock())
    }

    /// What the server has shown so far, for [`Self::worked_since`].
    pub(super) fn mark(&self) -> ActivityMark {
        ActivityMark {
            cpu: self.cpu_time(),
            signals: self.signals(),
            at: Instant::now(),
        }
    }

    /// Whether the server worked since `mark`: the same signs a request wait
    /// extends on (CPU time, a new work-done notice, any message). A token
    /// left open with nothing new said is none.
    pub(super) fn worked_since(&self, mark: &ActivityMark) -> bool {
        cpu_progressed(mark.cpu, self.cpu_time(), mark.at.elapsed())
            || self.signals() != mark.signals
    }
}

/// The server's activity at one instant ([`ServerActivity::mark`]).
pub(crate) struct ActivityMark {
    cpu: Option<Duration>,
    signals: (u64, u64),
    at: Instant,
}

/// `true` when CPU time grew by at least `step / CPU_PROGRESS_DIVISOR`
/// between two readable samples.
fn cpu_progressed(before: Option<Duration>, after: Option<Duration>, step: Duration) -> bool {
    match before.zip(after) {
        Some((before, after)) => after.saturating_sub(before) >= step / CPU_PROGRESS_DIVISOR,
        None => false,
    }
}
