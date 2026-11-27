//! The clients waiting for a stopping server to drain (`cache clear`): what
//! they are told, and that they keep hearing from it while the calls drain.

use super::Daemon;
use crate::cli::serve::unix::frames::{ALIVE, Frames};
use serde_json::{Value, json};
use tokio::net::unix::OwnedWriteHalf;

/// Clients waiting for the server to stop, each with its own writer.
pub(super) type Waiters = std::cell::RefCell<Vec<Frames<OwnedWriteHalf>>>;

/// Send `value` to each of `waiters`, at once; a waiter that is gone
/// ([`super::super::frames::Gone`]) leaves the list.
async fn tell_each(waiters: &mut Vec<Frames<OwnedWriteHalf>>, value: &Value) {
    let sent =
        futures_util::future::join_all(waiters.iter_mut().map(|waiter| waiter.send(value))).await;
    let mut sent = sent.into_iter();
    waiters.retain(|_| sent.next().is_some_and(|sent| sent.is_ok()));
}

impl Daemon {
    /// Add a client that waits for this server to stop.
    pub(super) fn wait_for_stop(&self, write: OwnedWriteHalf) {
        self.stopped
            .borrow_mut()
            .push(Frames::new(write, self.pace));
    }

    /// Send `value` to each waiting client.
    pub(super) async fn tell_waiters(&self, value: &Value) {
        let mut waiters = std::mem::take(&mut *self.stopped.borrow_mut());
        tell_each(&mut waiters, value).await;
        self.stopped.borrow_mut().extend(waiters);
    }

    /// Tell each waiting client how many calls it waits for, once, before
    /// the wait.
    pub(in crate::cli::serve::unix) async fn tell_draining(&self, in_flight: usize) {
        self.tell_waiters(&json!({ "draining": in_flight })).await;
    }

    /// Await `work` (the calls draining, the runtimes closing) while each
    /// waiting client hears a heartbeat frame every heartbeat of the pace:
    /// a client tells a long drain from a server that stopped answering,
    /// and the drain is never cut. A heartbeat cut short when the work ends
    /// is finished before the next line.
    pub(in crate::cli::serve::unix) async fn telling_waiters<T>(
        &self,
        work: impl std::future::Future<Output = T>,
    ) -> T {
        tokio::pin!(work);
        let beat = self.pace.heartbeat;
        let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + beat, beat);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut waiters = std::mem::take(&mut *self.stopped.borrow_mut());
        let alive = json!({ ALIVE: true });
        loop {
            let done = tokio::select! {
                biased;
                done = work.as_mut() => Some(done),
                () = async {
                    ticks.tick().await;
                    waiters.extend(std::mem::take(&mut *self.stopped.borrow_mut()));
                    tell_each(&mut waiters, &alive).await;
                } => None,
            };
            if let Some(done) = done {
                self.stopped.borrow_mut().extend(waiters);
                return done;
            }
        }
    }
}
