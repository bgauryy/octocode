//! The one request-cancellation signal: every tool, provider request and page
//! checks it, the MCP cancel path, CLI signals, runtime close and deadlines
//! raise it.
//!
//! A check reads atomics and takes no lock, so parallel workers that check
//! per item never contend. The wrapped tokio token only serves awaits
//! ([`Cancellation::cancelled`]); it is private, so nothing can cancel it
//! without also raising the flag a check reads.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio_util::sync::CancellationToken;

/// A request's cancellation. Clones share one signal. Nothing runs in the
/// background: dropping the last clone frees it.
#[derive(Clone, Debug)]
pub struct Cancellation {
    /// Raised by [`cancel`](Self::cancel).
    raised: Arc<AtomicBool>,
    /// Wakes awaits. Cancelled after `raised` is set, so a woken waiter's
    /// check always sees the cancellation.
    token: CancellationToken,
}

impl Default for Cancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancellation {
    /// A signal, not cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            raised: Arc::default(),
            token: CancellationToken::new(),
        }
    }

    /// Cancel this signal and its clones. Idempotent.
    pub fn cancel(&self) {
        self.raised.store(true, Ordering::Release);
        self.token.cancel();
    }

    /// Whether this signal was cancelled: one atomic load, no lock.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.raised.load(Ordering::Acquire)
    }

    /// Completes once this signal is cancelled.
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    /// A guard that cancels this signal when dropped.
    #[must_use]
    pub fn drop_guard(self) -> CancelOnDrop {
        CancelOnDrop(self)
    }
}

/// Cancels its [`Cancellation`] when dropped
/// ([`Cancellation::drop_guard`]).
#[derive(Debug)]
pub struct CancelOnDrop(Cancellation);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(test)]
mod tests;
