//! `$/progress` tracking and the readiness signal derived from it.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{Mutex, watch};
use tokio::time::{Duration, Instant};

/// Cap on concurrently-active `$/progress` begin tokens. A conformant server has a
/// handful of in-flight progress streams; beyond this a server is either buggy or
/// hostile, so extra begins are ignored rather than growing the set without bound.
const MAX_ACTIVE_PROGRESS_TOKENS: usize = 512;

/// Synthetic token for rust-analyzer's non-quiescent server status. The
/// `\0` prefix keeps it from colliding with any real `$/progress` token.
const SERVER_STATUS_TOKEN: &str = "\0octocode/serverStatus";

/// How `wait_until_idle` concluded — a readiness signal the LSP tool uses to
/// tell "server confirmed indexing is done" apart from "server never told us,
/// we only waited a fixed window" apart from "server is still busy".
///
/// This distinction is what lets a *zero-results* semantic query be reported
/// honestly: only `ProgressIdle` means the empty answer reflects the indexed
/// project; the other two mean the emptiness might just be "not indexed yet".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Readiness {
    /// Saw at least one `$/progress` cycle and drained it to idle — the server
    /// announced indexing and we waited for it to finish.
    ProgressIdle,
    /// Never saw any `$/progress`; only the settle window elapsed. Normal for
    /// servers that do not report indexing (e.g. typescript-language-server),
    /// so completion cannot be confirmed — not an error.
    SilentServer,
    /// `$/progress` was still active when `timeout_ms` expired — the server is
    /// (as far as we know) still indexing.
    Timeout,
}

impl Readiness {
    /// Stable string form.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ProgressIdle => "progressIdle",
            Self::SilentServer => "settledWithoutProgress",
            Self::Timeout => "timeout",
        }
    }
}

/// Wait this long for the very first `$/progress begin` after
/// `initialized` is sent.
///
/// This window has to absorb two very different server behaviours:
///   * Servers that announce indexing via `$/progress` — they emit a
///     `begin` within this window and we then drain to completion.
///   * Servers that index WITHOUT progress events — the only safe
///     signal we have is elapsed time, so the window must be long
///     enough that the server has plausibly finished its initial work
///     before we let the first query through.
///
/// 100 ms was too aggressive: a server indexing silently would race the
/// first query and return wrong/empty results. We use a conservative
/// few-second window instead, always bounded by the caller's
/// `timeout_ms` so `wait_for_ready` can never block longer than asked.
pub(crate) const STARTUP_SETTLE_MS: u64 = 2_000;

/// Tracks in-flight `$/progress` tokens emitted by a language server.
///
/// After `initialized` is sent, servers like `rust-analyzer` begin asynchronous
/// project indexing and announce it via `$/progress begin`/`end` notifications.
/// `wait_until_idle` gates on all such tokens completing (or a deadline firing).
///
/// Two-phase wait:
///   1. **Settle** ([`STARTUP_SETTLE_MS`]): wait for the first `begin` to arrive.
///      Servers that don't use progress return immediately after this window.
///   2. **Drain**: wait until every active token ends or the original deadline expires.
pub(crate) struct ProgressTracker {
    active: Mutex<HashSet<String>>,
    /// `true` once at least one `begin` notification has been received.
    ever_active: AtomicBool,
    /// Tokens that began, counted once per start of a token not already open.
    begins: AtomicU64,
    count_tx: watch::Sender<usize>,
    count_rx: watch::Receiver<usize>,
}

impl ProgressTracker {
    pub(crate) fn new() -> Arc<Self> {
        let (count_tx, count_rx) = watch::channel(0usize);
        Arc::new(Self {
            active: Mutex::new(HashSet::new()),
            ever_active: AtomicBool::new(false),
            begins: AtomicU64::new(0),
            count_tx,
            count_rx,
        })
    }

    pub(crate) async fn on_begin(&self, token: String) {
        let mut active = self.active.lock().await;
        // Cap the active set so a server emitting an unbounded stream of distinct
        // `begin` tokens (never matched by `end`) cannot grow memory or wedge
        // `wait_until_idle` forever. Re-inserting an already-tracked token is fine;
        // only genuinely new tokens beyond the cap are dropped.
        if !active.contains(&token) && active.len() >= MAX_ACTIVE_PROGRESS_TOKENS {
            return;
        }
        if active.insert(token) {
            self.begins.fetch_add(1, Ordering::AcqRel);
        }
        self.ever_active.store(true, Ordering::Release);
        let _ = self.count_tx.send(active.len());
    }

    pub(crate) async fn on_end(&self, token: &str) {
        let mut active = self.active.lock().await;
        active.remove(token);
        let _ = self.count_tx.send(active.len());
    }

    /// How many tokens have begun so far: a change between two reads means
    /// the server started work in between.
    pub(crate) fn begins(&self) -> u64 {
        self.begins.load(Ordering::Acquire)
    }

    /// `true` while any progress token (or rust-analyzer's non-quiescent
    /// status) is open: the server says it is working.
    pub(crate) fn has_active(&self) -> bool {
        *self.count_rx.borrow() > 0
    }

    /// rust-analyzer `experimental/serverStatus`: `quiescent:false` is treated
    /// as one open progress token and `quiescent:true` as its end, so readiness
    /// waits for rust-analyzer's own "done loading" signal as well as for
    /// `$/progress`. Only rust-analyzer sends it, and only when the client
    /// opted in with `experimental.serverStatusNotification`.
    pub(crate) async fn on_server_status(&self, quiescent: bool) {
        if quiescent {
            self.on_end(SERVER_STATUS_TOKEN).await;
        } else {
            self.on_begin(SERVER_STATUS_TOKEN.to_owned()).await;
        }
    }

    /// Blocks until all in-flight tokens end **and** a quiescence window passes
    /// with no new tokens starting, or until `timeout_ms` elapses.
    ///
    /// Servers like `rust-analyzer` emit several sequential `$/progress` waves
    /// (e.g. crate loading -> workspace analysis -> cache priming).  Without
    /// the quiescence window we would return after the *first* wave, before the
    /// server is fully ready to answer queries.
    /// `settle_ms` is the settle window (normally [`STARTUP_SETTLE_MS`]).
    pub(crate) async fn wait_until_idle(&self, timeout_ms: u64, settle_ms: u64) -> Readiness {
        /// After count reaches 0, wait this long for any follow-up wave before
        /// declaring the server idle.  Sized to bridge the typical gap between
        /// rust-analyzer progress waves (~10-100 ms in practice).
        const QUIESCE_MS: u64 = 200;

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        let mut rx = self.count_rx.clone();

        // Phase 1 -- settle: wait briefly for the first $/progress begin.
        if *rx.borrow() == 0 && !self.ever_active.load(Ordering::Acquire) {
            let settle = Duration::from_millis(settle_ms.min(timeout_ms));
            let became_active = tokio::time::timeout(settle, rx.wait_for(|c| *c > 0))
                .await
                .is_ok();
            if !became_active {
                // Server does not use progress -- we only waited the settle
                // window, so we cannot confirm indexing actually finished.
                return Readiness::SilentServer;
            }
        }

        Self::drain_until_quiet(&mut rx, deadline, QUIESCE_MS).await
    }

    /// Snapshot the progress stream so a later [`Self::wait_until_idle_after`]
    /// observes every `begin`/`end` that arrives from this point on — even one
    /// that starts and finishes before the waiter is polled.
    pub(crate) fn subscribe(&self) -> watch::Receiver<usize> {
        let mut rx = self.count_rx.clone();
        rx.borrow_and_update();
        rx
    }

    /// Like [`Self::wait_until_idle`] but scoped to progress that starts AFTER
    /// `rx` was taken via [`Self::subscribe`], with a caller-chosen settle.
    ///
    /// Servers such as `typescript-language-server` announce nothing on
    /// `initialized` and only start loading a project once a document is
    /// opened (`$/progress begin` ~100 ms after `didOpen`). Waiting here after
    /// the first `didOpen` keeps queries from racing that load. The settle is
    /// short because the triggering event is known; servers that never report
    /// progress pay only `settle_ms`.
    pub(crate) async fn wait_until_idle_after(
        &self,
        mut rx: watch::Receiver<usize>,
        settle_ms: u64,
        timeout_ms: u64,
    ) -> Readiness {
        const QUIESCE_MS: u64 = 200;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        if *rx.borrow() == 0 {
            let settle = Duration::from_millis(settle_ms.min(timeout_ms));
            // Any update since `subscribe` counts: a begin/end pair that already
            // completed still bumps the channel version.
            if tokio::time::timeout(settle, rx.changed()).await.is_err() {
                return Readiness::SilentServer;
            }
        }
        Self::drain_until_quiet(&mut rx, deadline, QUIESCE_MS).await
    }

    /// Drain + quiesce loop: repeat until a full `quiesce_ms` window passes
    /// with no active tokens and no new ones starting, or `deadline` expires.
    async fn drain_until_quiet(
        rx: &mut watch::Receiver<usize>,
        deadline: Instant,
        quiesce_ms: u64,
    ) -> Readiness {
        loop {
            // Wait for count to reach zero.
            let remaining = deadline.saturating_duration_since(Instant::now());
            if tokio::time::timeout(remaining, rx.wait_for(|c| *c == 0))
                .await
                .is_err()
            {
                return Readiness::Timeout; // Deadline expired while tokens active.
            }

            // Quiesce: wait briefly to see if a new wave starts.
            let remaining = deadline.saturating_duration_since(Instant::now());
            let full_quiescence = Duration::from_millis(quiesce_ms);
            let quiesce = full_quiescence.min(remaining);
            // Any update breaks quiescence, including a short begin/end wave
            // whose active count was coalesced back to zero before we woke.
            let new_wave = tokio::time::timeout(quiesce, rx.changed()).await.is_ok();
            if !new_wave {
                return if remaining >= full_quiescence {
                    Readiness::ProgressIdle
                } else {
                    Readiness::Timeout
                };
            }
            // A new wave started; loop back and drain it too.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn progress_tracker_caps_active_begin_tokens() {
        // A server that emits an unbounded stream of distinct begin tokens must
        // not grow the active set without limit.
        let tracker = ProgressTracker::new();
        for index in 0..(MAX_ACTIVE_PROGRESS_TOKENS + 50) {
            tracker.on_begin(format!("token-{index}")).await;
        }
        assert_eq!(*tracker.count_rx.borrow(), MAX_ACTIVE_PROGRESS_TOKENS);
    }

    /// How long `wait_until_idle` waits, and what it reports, on paused time.
    #[tokio::test(start_paused = true)]
    async fn progress_tracker_wait_is_bounded_by_settle_and_caller_timeout() {
        let cases = [
            // Never saw progress -> only the settle window elapsed.
            (
                "settle is bounded by the caller timeout",
                None,
                150,
                150,
                Readiness::SilentServer,
            ),
            // A server that indexes WITHOUT progress events gets the conservative
            // settle window, not an aggressive 100 ms one.
            (
                "full settle when no progress and ample timeout",
                None,
                10_000,
                2_000,
                Readiness::SilentServer,
            ),
            (
                "times out when a token never ends",
                Some("stuck"),
                300,
                300,
                Readiness::Timeout,
            ),
        ];
        for (case, token, timeout_ms, elapsed_ms, expected) in cases {
            let tracker = ProgressTracker::new();
            if let Some(token) = token {
                tracker.on_begin(token.to_owned()).await;
            }
            let start = Instant::now();
            let readiness = tracker.wait_until_idle(timeout_ms, STARTUP_SETTLE_MS).await;
            assert_eq!(start.elapsed(), Duration::from_millis(elapsed_ms), "{case}");
            assert_eq!(readiness, expected, "{case}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn progress_tracker_waits_for_active_token_then_returns_idle() {
        let tracker = ProgressTracker::new();
        let t = Arc::clone(&tracker);
        tokio::spawn(async move {
            t.on_begin("indexing".to_owned()).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            t.on_end("indexing").await;
        });
        let readiness = tracker.wait_until_idle(5_000, STARTUP_SETTLE_MS).await;
        assert_eq!(*tracker.count_rx.borrow(), 0);
        assert_eq!(readiness, Readiness::ProgressIdle);
    }

    #[tokio::test(start_paused = true)]
    async fn server_status_not_quiescent_holds_readiness_until_quiescent() {
        let tracker = ProgressTracker::new();
        let status = Arc::clone(&tracker);
        tokio::spawn(async move {
            status.on_server_status(false).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
            status.on_server_status(true).await;
        });
        let started = Instant::now();
        assert_eq!(
            tracker.wait_until_idle(5_000, STARTUP_SETTLE_MS).await,
            Readiness::ProgressIdle
        );
        assert!(started.elapsed() >= Duration::from_millis(500));

        // Quiescent with nothing open is not activity, and never goes negative.
        let tracker = ProgressTracker::new();
        tracker.on_server_status(true).await;
        assert_eq!(*tracker.count_rx.borrow(), 0);
        tracker.on_server_status(false).await;
        tracker.on_server_status(false).await;
        assert_eq!(
            *tracker.count_rx.borrow(),
            1,
            "one status token, not one per report"
        );
        assert_eq!(
            tracker.wait_until_idle(300, STARTUP_SETTLE_MS).await,
            Readiness::Timeout
        );
    }

    #[tokio::test(start_paused = true)]
    async fn progress_tracker_completed_cycle_still_requires_quiescence() {
        let tracker = ProgressTracker::new();
        tracker.on_begin("indexing".to_owned()).await;
        tracker.on_end("indexing").await;
        let started = Instant::now();
        let readiness = tracker.wait_until_idle(5_000, STARTUP_SETTLE_MS).await;
        assert_eq!(readiness, Readiness::ProgressIdle);
        assert!(started.elapsed() >= Duration::from_millis(200));
    }

    #[tokio::test(start_paused = true)]
    async fn progress_tracker_cannot_confirm_idle_without_full_quiescence_budget() {
        let tracker = ProgressTracker::new();
        tracker.on_begin("indexing".to_owned()).await;
        tracker.on_end("indexing").await;
        assert_eq!(
            tracker.wait_until_idle(20, STARTUP_SETTLE_MS).await,
            Readiness::Timeout
        );
    }

    #[tokio::test(start_paused = true)]
    async fn progress_tracker_quiescence_restarts_for_a_followup_wave() {
        let tracker = ProgressTracker::new();
        tracker.on_begin("first".to_owned()).await;
        tracker.on_end("first").await;
        let followup = Arc::clone(&tracker);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            followup.on_begin("second".to_owned()).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            followup.on_end("second").await;
        });
        let started = Instant::now();
        assert_eq!(
            tracker.wait_until_idle(2_000, STARTUP_SETTLE_MS).await,
            Readiness::ProgressIdle
        );
        assert!(started.elapsed() >= Duration::from_millis(450));
    }
}
