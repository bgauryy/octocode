//! Clocks tests move. [`paused`] runs a test on a runtime whose time
//! ([`TestClock`], the language-server code's [`LspClock`]) stands still
//! while any task can run and, once none can, jumps to the earliest deadline
//! a task sleeps until: the behavior of tokio's paused clock, without tokio's
//! `test-util` feature in any build. [`ManualWalkClock`] is a walk's
//! [`WalkClock`] that moves only when the test moves it.

use crate::lsp::clock::{LspClock, Timer, Wake};
use crate::scan_budget::{WalkClock, WalkNow};
use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use tokio::time::{Duration, Instant};

/// The time and who sleeps until when, by (deadline, registration).
struct Sleepers {
    now: Instant,
    next_id: u64,
    waiting: BTreeMap<(Instant, u64), Waker>,
}

impl Sleepers {
    /// Removes and returns the wakers of every sleeper due by `now`.
    fn take_due(&mut self) -> Vec<Waker> {
        let mut due = Vec::new();
        while let Some(entry) = self.waiting.first_entry() {
            if entry.key().0 > self.now {
                break;
            }
            due.push(entry.remove());
        }
        due
    }
}

type SharedSleepers = Arc<Mutex<Sleepers>>;

fn lock(sleepers: &SharedSleepers) -> MutexGuard<'_, Sleepers> {
    sleepers.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Sets the time to `to` (never back) and wakes every sleeper due by then.
fn advance_to(sleepers: &SharedSleepers, to: Instant) {
    let due = {
        let mut sleepers = lock(sleepers);
        sleepers.now = sleepers.now.max(to);
        sleepers.take_due()
    };
    for waker in due {
        waker.wake();
    }
}

/// The [`Timer`] behind a [`TestClock`].
struct ManualTimer(SharedSleepers);

impl Timer for ManualTimer {
    fn now(&self) -> Instant {
        lock(&self.0).now
    }

    fn sleep_until(&self, deadline: Instant) -> Wake {
        Box::pin(Sleep {
            sleepers: Arc::clone(&self.0),
            deadline,
            id: None,
        })
    }
}

/// Completes once the clock reaches `deadline`; registered while pending.
struct Sleep {
    sleepers: SharedSleepers,
    deadline: Instant,
    id: Option<u64>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        let mut sleepers = lock(&this.sleepers);
        if sleepers.now >= this.deadline {
            if let Some(id) = this.id.take() {
                sleepers.waiting.remove(&(this.deadline, id));
            }
            return Poll::Ready(());
        }
        let id = match this.id {
            Some(id) => id,
            None => {
                let id = sleepers.next_id;
                sleepers.next_id += 1;
                this.id = Some(id);
                id
            }
        };
        sleepers
            .waiting
            .insert((this.deadline, id), context.waker().clone());
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            lock(&self.sleepers).waiting.remove(&(self.deadline, id));
        }
    }
}

/// A test's clock: hand [`Self::lsp`] to the code under test, and read,
/// sleep and time out on it through its [`LspClock`] methods.
#[derive(Clone)]
pub(crate) struct TestClock {
    sleepers: SharedSleepers,
    lsp: LspClock,
}

impl std::ops::Deref for TestClock {
    type Target = LspClock;

    fn deref(&self) -> &LspClock {
        &self.lsp
    }
}

impl TestClock {
    fn new() -> Self {
        let sleepers = Arc::new(Mutex::new(Sleepers {
            now: Instant::now(),
            next_id: 0,
            waiting: BTreeMap::new(),
        }));
        let timer: Arc<dyn Timer> = Arc::new(ManualTimer(Arc::clone(&sleepers)));
        Self {
            sleepers,
            lsp: LspClock::from(timer),
        }
    }

    /// The clock to hand the code under test.
    pub(crate) fn lsp(&self) -> LspClock {
        self.lsp.clone()
    }

    /// Moves time forward by `by` in one jump, wakes every sleeper due by
    /// then, and yields once so they run (tokio's `time::advance`).
    pub(crate) async fn advance(&self, by: Duration) {
        advance_to(&self.sleepers, self.now() + by);
        tokio::task::yield_now().await;
    }

    /// Jumps to the earliest deadline anyone sleeps until, if any.
    fn advance_to_next(&self) {
        let next = lock(&self.sleepers)
            .waiting
            .keys()
            .next()
            .map(|(deadline, _)| *deadline);
        if let Some(deadline) = next {
            advance_to(&self.sleepers, deadline);
        }
    }
}

/// Blocking-pool threads alive: work under way that a task may be waiting on.
/// The pool keeps no idle thread, so a live thread is one with work, from the
/// moment it is spawned (its name is asked for on the spawning thread) until
/// it stops.
#[derive(Default)]
struct BlockingWork {
    alive: AtomicUsize,
    /// Wakes the runtime once the last one stops, so it parks again and
    /// time can move.
    ended: tokio::sync::Notify,
}

/// Runs `test` on a current-thread runtime with no tokio timer, on the
/// [`TestClock`] it hands in: time stands still while any task can run or
/// any `spawn_blocking` work is under way and, when the runtime would park
/// with neither, jumps to the earliest deadline a task sleeps until. That is
/// tokio's paused clock, which also never moves during blocking work. Code
/// under test that arms a tokio timer instead of its clock panics ("timers
/// are disabled").
pub(crate) fn paused<F, Fut>(test: F)
where
    F: FnOnce(TestClock) -> Fut,
    Fut: Future<Output = ()>,
{
    let clock = TestClock::new();
    let blocking = Arc::new(BlockingWork::default());
    let (idle, spawned, stopped, nudged) = (
        clock.clone(),
        Arc::clone(&blocking),
        Arc::clone(&blocking),
        Arc::clone(&blocking),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .thread_keep_alive(Duration::ZERO)
        .thread_name_fn(move || {
            spawned.alive.fetch_add(1, Ordering::SeqCst);
            "octocode-test-blocking".to_owned()
        })
        .on_thread_stop(move || {
            if stopped.alive.fetch_sub(1, Ordering::SeqCst) == 1 {
                stopped.ended.notify_one();
            }
        })
        .on_thread_park(move || {
            if blocking.alive.load(Ordering::SeqCst) == 0 {
                idle.advance_to_next();
            }
        })
        .build()
        .expect("test runtime");
    // Woken when blocking work ends: the runtime then parks again, where
    // time may move.
    runtime.spawn(async move {
        loop {
            nudged.ended.notified().await;
        }
    });
    runtime.block_on(test(clock));
}

/// A walk's clock that stands still until the test moves it, so a time
/// budget is spent at the same entry however slowly the machine runs.
#[derive(Clone)]
pub(crate) struct ManualWalkClock(Arc<Mutex<std::time::Instant>>);

impl ManualWalkClock {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(std::time::Instant::now())))
    }

    /// Moves the clock forward by `by`.
    pub(crate) fn advance(&self, by: Duration) {
        let mut now = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *now += by;
    }

    /// The clock to hand the walk or scan under test.
    pub(crate) fn walk_clock(&self) -> WalkClock {
        let now = Arc::clone(&self.0);
        let reading: Arc<WalkNow> =
            Arc::new(move || *now.lock().unwrap_or_else(PoisonError::into_inner));
        WalkClock::from(reading)
    }
}
