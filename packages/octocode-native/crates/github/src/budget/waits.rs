//! What a rate-limit wait says to the call's host while it lasts
//! ([`octocode_io::budget::pause_reporting`]): which limit, how many seconds
//! are left, and why the request is not sent yet.
use super::key_state::Block;
use super::{Group, ceil_secs};
use octocode_contract_types::{ClientWait, ClientWaitKind};
use std::sync::Mutex;
use std::time::Duration;

/// What one request waited on the client before its sends, summed by kind:
/// its row's `clientWaits`. A request's clones share it, so every wait of
/// one row lands here and no other row's does.
#[derive(Debug, Default)]
pub(crate) struct ClientWaits {
    own: Mutex<Vec<ClientWait>>,
    /// Waits of a shared request this row waited on (the batch warm-up),
    /// by kind: the latest view, never added twice.
    adopted: Mutex<Vec<ClientWait>>,
    /// The waits (by address) that adopted these first: a shared request's
    /// waits are stated once, on that row, not again on every row of the
    /// batch that read its answer.
    adopter: Mutex<Option<usize>>,
}

/// `ms` added to `kind` in `waits`, or `kind` entered with it.
fn add(waits: &mut Vec<ClientWait>, kind: ClientWaitKind, ms: i64) {
    match waits.iter_mut().find(|wait| wait.kind == kind) {
        Some(wait) => wait.ms = wait.ms.saturating_add(ms),
        None => waits.push(ClientWait { kind, ms }),
    }
}

impl ClientWaits {
    /// Add `waited` to `kind`; a wait under a millisecond is none.
    pub(crate) fn record(&self, kind: ClientWaitKind, waited: Duration) {
        let ms = i64::try_from(waited.as_millis()).unwrap_or(i64::MAX);
        if ms == 0 {
            return;
        }
        let mut waits = self
            .own
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        add(&mut waits, kind, ms);
    }

    /// Take in what `shared` (a request this row waited on) has waited so
    /// far. Its waits only grow, so the larger view per kind replaces the
    /// smaller: reading `shared` twice counts its waits once.
    ///
    /// Only the first row to adopt `shared` takes its waits in: the rows of
    /// one batch waited on the same warm-up, and saying so once is enough.
    ///
    /// Single source: a row adopts from one shared request only, its batch's
    /// warm-up, which it reads more than once (each ref, each path). The
    /// per-kind maximum is right for that one source re-read; two different
    /// sources adopted here would undercount, so a second source needs its
    /// own adopted set, summed beside this one.
    pub(crate) fn adopt(&self, shared: &Self) {
        if std::ptr::eq(self, shared) {
            return;
        }
        let me = std::ptr::from_ref(self) as usize;
        let first = *shared
            .adopter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert(me);
        if first != me {
            return;
        }
        let seen = shared.summed();
        let mut adopted = self
            .adopted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for wait in seen {
            match adopted.iter_mut().find(|known| known.kind == wait.kind) {
                Some(known) => known.ms = known.ms.max(wait.ms),
                None => adopted.push(wait),
            }
        }
    }

    /// Run `wait` and record the time it took as `kind`, whether it ended
    /// in time, at a deadline or at a cancel: the row waited either way.
    pub(crate) async fn timed<T>(
        &self,
        kind: ClientWaitKind,
        wait: impl std::future::Future<Output = T>,
    ) -> T {
        let started = std::time::Instant::now();
        let ended = wait.await;
        self.record(kind, started.elapsed());
        ended
    }

    /// The waits so far, one entry per kind, in the order first waited.
    pub(crate) fn summed(&self) -> Vec<ClientWait> {
        let mut waits = self
            .own
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for wait in self
            .adopted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
        {
            add(&mut waits, wait.kind, wait.ms);
        }
        waits
    }
}

/// What a wait for `block` is: a spent quota waits for its reset (or the
/// `retry-after` GitHub sent with it), anything else is a secondary-limit
/// cooldown.
pub(super) fn block_kind(block: &Block) -> ClientWaitKind {
    if block.reset.is_some() {
        ClientWaitKind::QuotaPacing
    } else {
        ClientWaitKind::SecondaryLimit
    }
}

/// A request held by a block: an exhausted bucket (its reset, or the
/// `retry-after` GitHub sent) or a secondary-limit cooldown.
pub(super) fn blocked(block: &Block, remaining: Duration) -> String {
    let seconds = ceil_secs(remaining);
    let resource = &block.resource;
    if block.reset.is_some() {
        format!(
            "GitHub {resource} limit reached: waiting {seconds}s for the quota to reset or the retry-after to pass"
        )
    } else {
        format!("GitHub secondary rate limit: waiting {seconds}s for the cooldown to end")
    }
}

/// A request spaced after the group's last start by the reported quota.
pub(super) fn paced(group: Group, remaining: Duration) -> String {
    format!(
        "GitHub {} requests are paced to the reported quota: waiting {}s for the next slot",
        group.name(),
        ceil_secs(remaining)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(reset: Option<u64>) -> Block {
        Block {
            until_ms: 0,
            resource: "code_search".into(),
            remaining: reset.map(|_| 0),
            reset,
            circuit: None,
        }
    }

    /// Each reason reads differently, and whole seconds round up.
    #[test]
    fn a_wait_names_its_limit_seconds_and_reason() {
        let left = Duration::from_millis(51_200);
        let quota = blocked(&block(Some(1)), left);
        assert!(
            quota.contains("code_search") && quota.contains("52s"),
            "{quota}"
        );
        assert!(quota.contains("quota to reset"), "{quota}");
        let cooldown = blocked(&block(None), left);
        assert!(
            cooldown.contains("secondary") && cooldown.contains("52s"),
            "{cooldown}"
        );
        let slot = paced(Group::CodeSearch, left);
        assert!(
            slot.contains("code_search") && slot.contains("52s"),
            "{slot}"
        );
    }
}
