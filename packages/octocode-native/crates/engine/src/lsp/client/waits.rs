//! What callers waited on at a client: its server process starting, and its
//! project load. A call that held a client reads the waits that overlapped
//! it ([`NativeLspClient::waits_since`]), so it can say what it waited for.

use super::NativeLspClient;
use std::collections::VecDeque;
use tokio::time::{Duration, Instant};

/// Waits kept per client. A wait is logged only when one happens (a start,
/// or a project load that was owed), never per request, so a warm client
/// logs nothing; the oldest entry goes first.
const KEPT_WAITS: usize = 16;

/// What a wait was on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WaitKind {
    /// The server process starting and answering `initialize`.
    Starting,
    /// The server loading or indexing the project (startup and first-open
    /// readiness waits, including their settle windows).
    Indexing,
}

#[derive(Debug, Default)]
pub(super) struct Waits {
    log: VecDeque<(WaitKind, Instant, Instant)>,
}

impl Waits {
    fn record(&mut self, kind: WaitKind, began: Instant, ended: Instant) {
        if self.log.len() == KEPT_WAITS {
            self.log.pop_front();
        }
        self.log.push_back((kind, began, ended));
    }

    /// The time of `kind` that passed after `since`.
    fn since(&self, kind: WaitKind, since: Instant) -> Option<Duration> {
        self.log
            .iter()
            .filter(|(logged, _, _)| *logged == kind)
            .map(|(_, began, ended)| ended.saturating_duration_since((*began).max(since)))
            .filter(|waited| !waited.is_zero())
            .reduce(|total, waited| total + waited)
    }
}

/// What a client's callers waited on during a period.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServerWaits {
    /// Time the server process took to start.
    pub starting: Option<Duration>,
    /// Time the server took to load or index the project.
    pub indexing: Option<Duration>,
}

impl NativeLspClient {
    /// What happened at this client after `since`: the part of each start
    /// and project-load wait that fell after it. Empty for a warm client
    /// that had nothing to wait on.
    pub fn waits_since(&self, since: Instant) -> ServerWaits {
        let waits = self
            .inner
            .waits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ServerWaits {
            starting: waits.since(WaitKind::Starting, since),
            indexing: waits.since(WaitKind::Indexing, since),
        }
    }

    pub(super) fn record_wait(&self, kind: WaitKind, began: Instant) {
        self.inner
            .waits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record(kind, began, Instant::now());
    }
}
