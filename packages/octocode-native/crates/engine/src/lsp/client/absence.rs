//! What an answer with no use in it can claim (empty, or only the declaration
//! it can see). A server that has not finished loading may answer "nothing
//! here" about a name it simply has not reached, and the
//! readiness waits cannot rule that out: a settle window ends whether or not
//! the load has begun (on a loaded machine Metals announced its first progress
//! 2-5 s after `initialized`, past both settles, then answered `[]`).
//!
//! So an empty answer is judged when it arrives, by what the server shows:
//! an open `$/progress` token, progress that began since the question was
//! asked, or a process tree still working. Only a server that shows none of
//! these has reached a state where the answer means absence. Answers that
//! list a use claim no absence and wait for nothing.

use super::{NativeLspClient, OPEN_READY_TIMEOUT_MS, RESPONSE_SCOPE};
use crate::lsp::transport::{JsonRpcConnection, Readiness, WaitBudget};
use std::sync::atomic::Ordering;
use tokio::time::{Duration, Instant};

/// How long one look at the server's CPU lasts.
const CPU_SAMPLE: Duration = Duration::from_millis(500);
/// A server using this share of one core or more is working. Measured idle,
/// every server stays under 2% (tsgo's runtime ticks, the JVM's housekeeping
/// in Metals and jdtls), and a starved one mid-load well over it.
const WORKING_CPU_DIVISOR: u32 = 20;

/// Whether `deadline` (none: never) has come by `now`.
fn past(deadline: Option<Instant>, now: Instant) -> bool {
    deadline.is_some_and(|at| now >= at)
}

/// What an empty answer means, as far as the server shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyAnswer {
    /// The server shows no load at or since the question: the answer stands.
    Stands,
    /// The server worked since the question: its answer is out of date.
    AskAgain,
    /// The server was still loading when the wait ended: the caller's
    /// deadline came while it worked, or it held a load open with no sign
    /// of work for a whole quiet window.
    StillLoading,
}

/// Progress that had begun, and whether it was still active, when a question was asked.
#[derive(Clone, Copy, Debug)]
pub struct AskedAt {
    begins: u64,
    active: bool,
}

impl NativeLspClient {
    /// The mark to take before asking, for [`Self::judge_empty_answer`].
    pub async fn asked_at(&self) -> AskedAt {
        let (begins, active) = self.inner.progress.work_state().await;
        AskedAt { begins, active }
    }

    /// When a request's waits for its server's load end: the caller's
    /// deadline (less the reserve its failure row needs), none for a caller
    /// with no deadline, whose cancel ends them; a wait no caller bounds gets
    /// the language's readiness budget, twice. Within it a load is waited for
    /// as long as the server shows work, never a fixed span.
    pub fn load_deadline(&self) -> Option<Instant> {
        WaitBudget::new(self.load_quiet_ms(), self.inner.clock.now()).ceiling()
    }

    /// How long a load may show no sign of work (no CPU, no new work-done
    /// notice, no message) before an open token stops counting as work: the
    /// language's readiness budget. It bounds silence, never work.
    fn load_quiet_ms(&self) -> u32 {
        crate::lsp::pool::readiness_timeout(self.language_id()).unwrap_or(OPEN_READY_TIMEOUT_MS)
    }

    /// Judge an empty answer to a question asked at `asked`. Waits, up to
    /// `deadline` (none: until the caller cancels), only while the server
    /// shows it has work left: an open `$/progress` token whose server keeps
    /// working (CPU, a new notice, any message) or a process tree using CPU.
    /// A server already shown idle when asked costs nothing; a token held
    /// open with no sign of work for a whole quiet window
    /// ([`Self::load_quiet_ms`]) is a load that shows none, which ends the
    /// wait. Asking again forgets the answers the call recorded: they
    /// predate the load.
    pub async fn judge_empty_answer(
        &self,
        asked: AskedAt,
        deadline: Option<Instant>,
    ) -> EmptyAnswer {
        let progress = &self.inner.progress;
        let clock = &self.inner.clock;
        let _activity = self.lease();
        // Shown idle when asked, with nothing open now: whatever began since
        // has ended, and a server may wrap a request in its own progress
        // (pyright's "Finding references"), which is no load.
        let (begun, active) = progress.work_state().await;
        if !asked.active
            && !active
            && self.inner.idle_at_begin.load(Ordering::Acquire) == asked.begins + 1
        {
            self.inner.idle_at_begin.store(begun + 1, Ordering::Release);
            return EmptyAnswer::Stands;
        }
        let mut worked = asked.active || begun != asked.begins;
        let Ok(connection) = self.connection_handle().await else {
            // The server is gone: the next request reports that.
            return EmptyAnswer::Stands;
        };
        let sample = self.settle(CPU_SAMPLE);
        loop {
            if progress.has_active() {
                worked = true;
                if !self.outwait_load(&connection, deadline).await {
                    return EmptyAnswer::StillLoading;
                }
            }
            let begins = progress.begins();
            let before = connection.server_cpu_time();
            clock.sleep(sample).await;
            // No window (a harness's fake server), no evidence of work.
            let working = !sample.is_zero()
                && before
                    .zip(connection.server_cpu_time())
                    .is_some_and(|(before, after)| {
                        after.saturating_sub(before) >= sample / WORKING_CPU_DIVISOR
                    });
            if progress.begins() == begins && !progress.has_active() && !working {
                self.inner
                    .idle_at_begin
                    .store(begins + 1, Ordering::Release);
                break;
            }
            worked = true;
            if past(deadline, clock.now()) {
                return EmptyAnswer::StillLoading;
            }
        }
        if !worked {
            return EmptyAnswer::Stands;
        }
        if past(deadline, clock.now()) {
            return EmptyAnswer::StillLoading;
        }
        let _ = RESPONSE_SCOPE.try_with(|scope| {
            if let Some(answers) = &scope.borrow().answers {
                answers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0
                    .clear();
            }
        });
        EmptyAnswer::AskAgain
    }

    /// Waits a load the startup readiness wait left under way
    /// ([`Readiness::Timeout`]) out, for as long as the server works, within
    /// `deadline` (none: until the caller cancels), and stores how it ended:
    /// a server whose load ended is ready for the request, one still loading
    /// stays `Timeout`. Any other readiness returns at once.
    pub async fn finish_load(&self, deadline: Option<Instant>) -> Option<Readiness> {
        if self.readiness() != Some(Readiness::Timeout) {
            return self.readiness();
        }
        let _activity = self.lease();
        let Ok(connection) = self.connection_handle().await else {
            return self.readiness();
        };
        let ended = self.outwait_load(&connection, deadline).await;
        if ended {
            *self
                .inner
                .readiness
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Readiness::ProgressIdle);
        }
        self.readiness()
    }

    /// Waits while a load is open and the server works: `true` once no token
    /// is open, `false` at `deadline` or after a whole quiet window
    /// ([`Self::load_quiet_ms`]) in which the server showed no sign of work.
    async fn outwait_load(
        &self,
        connection: &JsonRpcConnection,
        deadline: Option<Instant>,
    ) -> bool {
        let progress = &self.inner.progress;
        let clock = &self.inner.clock;
        let quiet = Duration::from_millis(u64::from(self.load_quiet_ms()));
        while progress.has_active() {
            let mark = connection.activity_mark();
            let window = clock.deadline_after(quiet);
            let window = deadline.map_or(window, |at| window.min(at));
            let left = window.saturating_duration_since(clock.now());
            let left = u64::try_from(left.as_millis()).unwrap_or(u64::MAX);
            let drained = progress
                .wait_until_idle_after(progress.subscribe(), 0, left)
                .await;
            if drained != Readiness::Timeout {
                return true;
            }
            if past(deadline, clock.now()) || !connection.worked_since(&mark) {
                return false;
            }
        }
        true
    }
}
