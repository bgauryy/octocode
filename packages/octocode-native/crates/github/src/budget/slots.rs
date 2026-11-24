//! Slot planning and admission lanes: when a group's next start may run,
//! the admissions queued ahead, and reservations that hand themselves back.
use super::key_state::KeyFacts;
use super::{ExecutorConfig, Group, KeyState, ceil_secs, rate_limited_error};
use crate::{ProviderError, RateLimit};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

/// When one request's client-side waits end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WaitLimit {
    /// The request's deadline: it ends every wait under way (a permit, a
    /// lock, a sleep already decided).
    pub(crate) deadline: Instant,
    /// Where a new sleep must end before: `deadline`, or the request's
    /// sooner wait bound ([`crate::RequestContext::with_wait_bound`]).
    pub(crate) sleep_by: Instant,
}

impl From<Instant> for WaitLimit {
    /// A request with no wait bound: its deadline bounds its sleeps.
    fn from(deadline: Instant) -> Self {
        Self {
            deadline,
            sleep_by: deadline,
        }
    }
}

impl WaitLimit {
    /// Whether a wait of `wait` from now is taken: it ends before
    /// `sleep_by`. No wait at all sleeps nothing, so only the request's own
    /// deadline refuses it: past the wait bound a request still sends what
    /// needs no wait.
    pub(crate) fn takes(&self, wait: Duration) -> bool {
        let by = if wait.is_zero() {
            self.deadline
        } else {
            self.sleep_by
        };
        // Compared as time left, so a provider wait of any length adds to
        // no instant.
        wait < by.saturating_duration_since(Instant::now())
    }

    /// The part of `wait` slept when the wait bound, not the deadline,
    /// refuses it whole: the time left to the bound. The bound stands for a
    /// host that stops listening (an MCP reply budget), not for a caller
    /// that will not wait, and an agent cannot sleep: a refusal that slept
    /// nothing hands back a retry that is refused the same way. Sleeping up
    /// to the bound leaves the retry only the rest, which its own bound
    /// fits. `None` when the deadline refuses the wait too, or no time is
    /// left to the bound.
    pub(crate) fn part_within_bound(&self, wait: Duration) -> Option<Duration> {
        let now = Instant::now();
        let to_bound = self.sleep_by.saturating_duration_since(now);
        let to_deadline = self.deadline.saturating_duration_since(now);
        (self.sleep_by < self.deadline
            && !to_bound.is_zero()
            && to_bound <= wait
            && wait < to_deadline)
            .then_some(to_bound)
    }
}

/// One group's slot rules for an admission: the spacing after its last start
/// (derived from the reported or assumed quota when the group is
/// quota-paced) and the limit bounding the wait. A start is refused when its
/// wait is not taken ([`WaitLimit::takes`]); one request's start past the
/// wait bound but before the deadline is slept to the bound first
/// ([`SlotPlan::fit`]).
pub(super) struct SlotPlan<'a> {
    pub(super) group: Group,
    pub(super) config: &'a ExecutorConfig,
    /// Requests this admission plans together (one for an ordinary request).
    pub(super) planned: usize,
    pub(super) limit: WaitLimit,
}

/// When a slot could start, measured from `now`.
#[derive(Clone, Copy)]
pub(super) struct PlannedStart {
    pub(super) start: u64,
    pub(super) now: u64,
}

impl PlannedStart {
    /// The wait from `now` to the start.
    pub(super) fn wait(self) -> Duration {
        Duration::from_millis(self.start.saturating_sub(self.now))
    }
}

/// What a refusal is about: one admission, or a row of `n` code searches
/// checked before its first is sent.
#[derive(Clone, Copy)]
pub(super) enum Subject {
    Request,
    Batch(usize),
}

/// How one request's planned start fits its [`WaitLimit`].
pub(super) enum Fit {
    /// Its wait ends before the wait bound: slept whole.
    Whole,
    /// Its wait ends past the wait bound but before the deadline: slept up
    /// to the bound (`part`), then refused with the rest, which the retry
    /// it hands back fits ([`WaitLimit::part_within_bound`]).
    ToBound { part: Duration },
    /// Its wait ends past the deadline: refused at once.
    Refused(ProviderError),
}

impl<'a> SlotPlan<'a> {
    pub(super) fn new(
        group: Group,
        config: &'a ExecutorConfig,
        planned: usize,
        limit: WaitLimit,
    ) -> Self {
        Self {
            group,
            config,
            planned,
            limit,
        }
    }

    /// Whether admissions under this plan wait for a slot at all.
    pub(super) fn takes_slot(&self) -> bool {
        self.config.paces(self.group)
    }

    /// The earliest start these facts allow, without taking it: the peek and
    /// the reservation decide by the same rule.
    pub(super) fn start(&self, facts: &KeyFacts, now: u64) -> PlannedStart {
        self.earliest_with_planned(facts, now, self.planned)
    }

    fn earliest_with_planned(&self, facts: &KeyFacts, now: u64, planned: usize) -> PlannedStart {
        let mut start = now;
        let spacing = self.config.spacing_ms(self.group, facts, now, planned);
        if let Some(last) = facts.last_start_ms.get(self.group.name()) {
            start = start.max(last.saturating_add(spacing));
        }
        PlannedStart { start, now }
    }

    /// This plan's start once the `ahead` slots queued before it are taken,
    /// simulated on a copy of the facts (the reservation stays exact).
    pub(super) fn start_after(&self, facts: &KeyFacts, now: u64, ahead: usize) -> PlannedStart {
        if ahead == 0 {
            return self.start(facts, now);
        }
        let mut facts = facts.clone();
        for _ in 0..ahead {
            self.take(&mut facts, now, self.planned);
        }
        self.start(&facts, now)
    }

    /// Simulate one reservation on a cloned facts view.
    fn take(&self, facts: &mut KeyFacts, now: u64, planned: usize) {
        let taken = self.earliest_with_planned(facts, now, planned);
        facts
            .last_start_ms
            .insert(self.group.name().to_owned(), taken.start);
    }

    /// The last start in a planned batch. Queued requests go first. The
    /// reported (or assumed) quota covers a prefix that bursts; only the
    /// excess is spaced, over the same reset window the reservation rule uses.
    pub(super) fn batch_start(&self, facts: &KeyFacts, now: u64, ahead: usize) -> PlannedStart {
        let mut facts = facts.clone();
        for _ in 0..ahead {
            self.take(&mut facts, now, 1);
        }
        let covered = if self.config.quota_pacing && self.group.quota_paced() {
            facts.quota(self.group, now).map_or(0, |bucket| {
                usize::try_from(bucket.remaining).unwrap_or(usize::MAX)
            })
        } else {
            0
        };
        let covered = covered.saturating_sub(ahead);
        for index in 0..self.planned {
            let planned = if index < covered { 1 } else { self.planned };
            self.take(&mut facts, now, planned);
        }
        self.start(&facts, now)
    }

    /// The refusal for a start whose wait is not taken, labelled by its
    /// subject: a row of searches is refused before its first, never slept
    /// to the bound, since a refusal midway would spend quota on a page that
    /// is then discarded.
    pub(super) fn refusal(&self, planned: PlannedStart, subject: Subject) -> Option<ProviderError> {
        let wait = planned.wait();
        (!self.limit.takes(wait)).then(|| self.refused(wait, subject))
    }

    /// How one request's planned start fits the limit ([`Fit`]).
    pub(super) fn fit(&self, planned: PlannedStart) -> Fit {
        let wait = planned.wait();
        if self.limit.takes(wait) {
            return Fit::Whole;
        }
        match self.limit.part_within_bound(wait) {
            Some(part) => Fit::ToBound { part },
            None => Fit::Refused(self.refused(wait, Subject::Request)),
        }
    }

    /// The refusal of a start `wait` away, labelled by its subject. It
    /// carries only what the client decided, the wait: GitHub reported no
    /// exhausted bucket, so no `remaining` or reset is claimed. The wait
    /// limit is the request's wait bound or its deadline, whichever refused.
    pub(super) fn refused(&self, wait: Duration, subject: Subject) -> ProviderError {
        let group = self.group.name();
        let message = match subject {
            Subject::Request => {
                format!(
                    "GitHub {group} request spacing passes this call's wait limit; request not sent."
                )
            }
            Subject::Batch(searches) => format!(
                "GitHub {group} request spacing cannot fit this row's {searches} searches within this call's wait limit; requests not sent."
            ),
        };
        rate_limited_error(
            &message,
            RateLimit {
                remaining: None,
                reset_epoch_seconds: None,
                retry_after_seconds: Some(ceil_secs(wait)),
                resource: Some(self.group.bucket().into()),
            },
        )
    }
}

/// One group's admission lane: the permit that serializes its requests, and
/// the admissions that passed the peek but have not reserved a slot yet.
pub(super) struct Lane {
    pub(super) permit: Arc<Semaphore>,
    pub(super) queued: Arc<AtomicUsize>,
}

/// One admission queued in a [`Lane`]; it leaves the count when dropped
/// (reserved, refused or cancelled). Entering and reserving both happen under
/// the key's facts lock, so a peek sees each queued admission either in the
/// count or as a slot in the facts, never both.
pub(super) struct Queued {
    counts: Arc<AtomicUsize>,
}

impl Queued {
    /// Join the lane's queue, returning the admissions queued ahead. Call
    /// with the key's facts locked.
    pub(super) fn enter(lane: &Lane) -> (Self, usize) {
        let ahead = lane.queued.fetch_add(1, Ordering::SeqCst);
        let queued = Self {
            counts: Arc::clone(&lane.queued),
        };
        (queued, ahead)
    }
}

impl Drop for Queued {
    fn drop(&mut self) {
        self.counts.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `(group, start, previous start, mirrored to disk)`.
pub(super) type ReservedSlot = (Group, u64, Option<u64>, bool);

/// A reserved start that hands itself back unless kept: an explicit failure
/// awaits the release, and a dropped admission future (a timeout wrapper, a
/// `select!` that lost) releases it from `Drop`.
pub(super) struct Reserved {
    pub(super) state: Arc<KeyState>,
    pub(super) slot: Option<ReservedSlot>,
}

impl Reserved {
    /// The request was admitted: the slot stays taken.
    pub(super) fn keep(mut self) {
        self.slot = None;
    }

    /// Hand the slot back and wait for the disk copy.
    pub(super) async fn release(mut self) {
        if let Some((group, start, previous, persisted)) = self.slot.take() {
            self.state
                .release_reservation(group, start, previous, persisted)
                .await;
        }
    }
}

impl Drop for Reserved {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.take() {
            self.state.release_reservation_detached(slot);
        }
    }
}
