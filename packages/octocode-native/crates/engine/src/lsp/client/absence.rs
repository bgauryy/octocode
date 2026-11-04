//! What an empty answer can claim. A server that has not finished loading may
//! answer "nothing here" about a name it simply has not reached, and the
//! readiness waits cannot rule that out: a settle window ends whether or not
//! the load has begun (on a loaded machine Metals announced its first progress
//! 2-5 s after `initialized`, past both settles, then answered `[]`).
//!
//! So an empty answer is judged when it arrives, by what the server shows:
//! an open `$/progress` token, progress that began since the question was
//! asked, or a process tree still working. Only a server that shows none of
//! these has reached a state where the answer means absence. Answers with
//! locations claim no absence and wait for nothing.

use super::status::RESUME_REWAIT_MS;
use super::{NativeLspClient, OPEN_READY_TIMEOUT_MS, RESPONSE_SCOPE};
use crate::lsp::transport::{Readiness, WaitBudget};
use std::sync::atomic::Ordering;
use tokio::time::{Duration, Instant};

/// How long one look at the server's CPU lasts.
const CPU_SAMPLE: Duration = Duration::from_millis(500);
/// A server using this share of one core or more is working. Measured idle,
/// every server stays under 2% (tsgo's runtime ticks, the JVM's housekeeping
/// in Metals and jdtls), and a starved one mid-load well over it.
const WORKING_CPU_DIVISOR: u32 = 20;

/// What an empty answer means, as far as the server shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyAnswer {
    /// The server shows no load at or since the question: the answer stands.
    Stands,
    /// The server worked since the question: its answer is out of date.
    AskAgain,
    /// The server was still working when the budget ended.
    StillLoading,
}

/// How much progress had begun when a question was asked.
#[derive(Clone, Copy, Debug)]
pub struct AskedAt(u64);

impl NativeLspClient {
    /// The mark to take before asking, for [`Self::judge_empty_answer`].
    pub fn asked_at(&self) -> AskedAt {
        AskedAt(self.inner.progress.begins())
    }

    /// When a request's waits for its server's load end: the language's
    /// readiness budget from now, within the caller's deadline. Once a load
    /// has outlasted that budget, a later call waits only
    /// [`RESUME_REWAIT_MS`] for it, so a token that never closes costs each
    /// call a bounded look, not the whole budget again.
    pub fn load_deadline(&self) -> Instant {
        let budget = crate::lsp::pool::readiness_timeout(self.language_id())
            .unwrap_or(OPEN_READY_TIMEOUT_MS);
        let budget = if self.inner.load_outlasted.load(Ordering::Acquire) {
            budget.min(RESUME_REWAIT_MS)
        } else {
            budget
        };
        WaitBudget::new(budget).first_cut()
    }

    /// Judge an empty answer to a question asked at `asked`. Waits, up to
    /// `deadline`, only until the server shows it has nothing left to do; a
    /// server already shown idle when asked costs nothing. Asking again
    /// forgets the answers the call recorded: they predate the load.
    pub async fn judge_empty_answer(&self, asked: AskedAt, deadline: Instant) -> EmptyAnswer {
        let progress = &self.inner.progress;
        let began = Instant::now();
        let _activity = self.lease();
        // Shown idle when asked, with nothing open now: whatever began since
        // has ended, and a server may wrap a request in its own progress
        // (pyright's "Finding references"), which is no load.
        let begun = progress.begins();
        if !progress.has_active() && self.inner.idle_at_begin.load(Ordering::Acquire) == asked.0 + 1
        {
            self.inner.idle_at_begin.store(begun + 1, Ordering::Release);
            return EmptyAnswer::Stands;
        }
        let mut worked = begun != asked.0;
        let Ok(connection) = self.connection_handle().await else {
            // The server is gone: the next request reports that.
            return EmptyAnswer::Stands;
        };
        let sample = self.settle(CPU_SAMPLE);
        loop {
            if progress.has_active() {
                worked = true;
                let left = deadline.saturating_duration_since(Instant::now());
                let left = u64::try_from(left.as_millis()).unwrap_or(u64::MAX);
                let drained = progress
                    .wait_until_idle_after(progress.subscribe(), 0, left)
                    .await;
                if drained == Readiness::Timeout {
                    return self.still_loading(began);
                }
            }
            let begins = progress.begins();
            let before = connection.server_cpu_time();
            tokio::time::sleep(sample).await;
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
                self.inner.load_outlasted.store(false, Ordering::Release);
                break;
            }
            worked = true;
            if Instant::now() >= deadline {
                return self.still_loading(began);
            }
        }
        self.record_wait(super::waits::WaitKind::Indexing, began);
        if !worked {
            return EmptyAnswer::Stands;
        }
        if Instant::now() >= deadline {
            return self.still_loading(began);
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

    /// The load outlasted the budget. The readiness state stays as it is: a
    /// server with a long-lived token still answers what it can, and only an
    /// empty answer waits.
    fn still_loading(&self, began: Instant) -> EmptyAnswer {
        self.record_wait(super::waits::WaitKind::Indexing, began);
        self.inner.load_outlasted.store(true, Ordering::Release);
        EmptyAnswer::StillLoading
    }
}
