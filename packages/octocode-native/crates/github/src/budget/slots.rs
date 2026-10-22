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

/// One group's slot rules for an admission: the spacing after its last start,
/// the code-search window, and the one cap and deadline bounding the wait.
pub(super) struct SlotPlan {
    pub(super) group: Group,
    pub(super) spacing: u64,
    pub(super) window: bool,
    pub(super) per_minute: usize,
    pub(super) cap: Duration,
    pub(super) deadline: Instant,
}

/// When a slot could start, measured from `now`.
#[derive(Clone, Copy)]
pub(super) struct PlannedStart {
    pub(super) start: u64,
    pub(super) now: u64,
}

/// What a refusal is about: one admission, or a row of `n` code searches
/// checked before its first is sent.
#[derive(Clone, Copy)]
pub(super) enum Subject {
    Request,
    Batch(usize),
}

impl SlotPlan {
    /// `window`: this admission takes a code-search window slot.
    pub(super) fn new(
        group: Group,
        config: &ExecutorConfig,
        window: bool,
        cap: Duration,
        deadline: Instant,
    ) -> Self {
        Self {
            group,
            spacing: config.spacing(group).as_millis() as u64,
            window: window && config.code_search_per_minute != usize::MAX,
            per_minute: config.code_search_per_minute,
            cap,
            deadline,
        }
    }

    /// Whether admissions under this plan wait for a slot at all.
    pub(super) fn takes_slot(&self) -> bool {
        self.spacing > 0 || self.window
    }

    /// The earliest start these facts allow for a slot that does (or does
    /// not) take a window slot, without taking it: the peek and the
    /// reservation decide by the same rule.
    pub(super) fn earliest(&self, facts: &KeyFacts, now: u64, window: bool) -> PlannedStart {
        let mut start = now;
        if let Some(last) = facts.last_start_ms.get(self.group.name()) {
            start = start.max(last.saturating_add(self.spacing));
        }
        if window {
            let live = || {
                facts
                    .code_search_ms
                    .iter()
                    .filter(|stamp| stamp.saturating_add(60_000) > now)
            };
            if live().count() >= self.per_minute
                && let Some(oldest) = live().next()
            {
                start = start.max(oldest.saturating_add(60_000));
            }
        }
        PlannedStart { start, now }
    }

    /// This plan's earliest start.
    pub(super) fn start(&self, facts: &KeyFacts, now: u64) -> PlannedStart {
        self.earliest(facts, now, self.window)
    }

    /// This plan's start once the slots `ahead` of it are taken, simulated on
    /// a copy of the facts. The order of plain and windowed slots ahead is
    /// not known; plain slots are placed first, so the peek never refuses a
    /// start some queue order could reach (the reservation stays exact).
    pub(super) fn start_after(&self, facts: &KeyFacts, now: u64, ahead: Ahead) -> PlannedStart {
        if ahead.plain + ahead.windowed == 0 {
            return self.start(facts, now);
        }
        let mut facts = facts.clone();
        let slots = std::iter::repeat_n(false, ahead.plain)
            .chain(std::iter::repeat_n(true, ahead.windowed));
        for window in slots {
            let taken = self.earliest(&facts, now, window).start;
            facts
                .last_start_ms
                .insert(self.group.name().to_owned(), taken);
            if window {
                facts.code_search_ms.push_back(taken);
            }
        }
        self.start(&facts, now)
    }

    /// The refusal for a start past the cap or the deadline, labelled by its
    /// cause and subject. It carries only what the client decided, the wait:
    /// GitHub reported no exhausted bucket, so no `remaining` or reset is
    /// claimed.
    pub(super) fn refusal(&self, planned: PlannedStart, subject: Subject) -> Option<ProviderError> {
        let wait = Duration::from_millis(planned.start.saturating_sub(planned.now));
        if wait <= self.cap.max(Duration::from_millis(self.spacing))
            && Instant::now() + wait < self.deadline
        {
            return None;
        }
        let group = self.group.name();
        let message = match (self.window, subject) {
            (true, Subject::Request) => {
                "GitHub code search window is full; request not sent.".to_owned()
            }
            (true, Subject::Batch(searches)) => format!(
                "GitHub code search window cannot fit this row's {searches} searches; requests not sent."
            ),
            (false, Subject::Request) => {
                format!("GitHub {group} request spacing exceeds the deadline; request not sent.")
            }
            (false, Subject::Batch(searches)) => format!(
                "GitHub {group} request spacing cannot fit this row's {searches} searches before the deadline; requests not sent."
            ),
        };
        Some(rate_limited_error(
            &message,
            RateLimit {
                remaining: None,
                reset_epoch_seconds: None,
                retry_after_seconds: Some(ceil_secs(wait)),
                resource: Some(self.group.bucket().into()),
            },
        ))
    }
}

/// One group's admission lane: the permit that serializes its requests, and
/// the admissions that passed the peek but have not reserved a slot yet.
pub(super) struct Lane {
    pub(super) permit: Arc<Semaphore>,
    pub(super) queued: Arc<QueueCounts>,
}

/// Admissions queued in a [`Lane`], split by whether they take a code-search
/// window slot. Entering and reserving both happen under the key's facts
/// lock, so a peek sees each queued admission either here or as a slot in the
/// facts, never both.
#[derive(Default)]
pub(super) struct QueueCounts {
    pub(super) plain: AtomicUsize,
    pub(super) windowed: AtomicUsize,
}

impl QueueCounts {
    pub(super) fn counter(&self, window: bool) -> &AtomicUsize {
        if window { &self.windowed } else { &self.plain }
    }

    pub(super) fn ahead(&self) -> Ahead {
        Ahead {
            plain: self.plain.load(Ordering::SeqCst),
            windowed: self.windowed.load(Ordering::SeqCst),
        }
    }
}

/// Slots that admissions queued ahead will take first.
#[derive(Clone, Copy, Default)]
pub(super) struct Ahead {
    pub(super) plain: usize,
    pub(super) windowed: usize,
}

impl Ahead {
    pub(super) fn add(&mut self, window: bool, slots: usize) {
        if window {
            self.windowed += slots;
        } else {
            self.plain += slots;
        }
    }
}

/// One admission queued in a [`Lane`]; it leaves the count when dropped
/// (reserved, refused or cancelled).
pub(super) struct Queued {
    pub(super) counts: Arc<QueueCounts>,
    pub(super) window: bool,
}

impl Queued {
    /// Join the lane's queue, returning the slots queued ahead. Call with the
    /// key's facts locked.
    pub(super) fn enter(lane: &Lane, window: bool) -> (Self, Ahead) {
        let ahead = lane.queued.ahead();
        lane.queued.counter(window).fetch_add(1, Ordering::SeqCst);
        let queued = Self {
            counts: Arc::clone(&lane.queued),
            window,
        };
        (queued, ahead)
    }
}

/// `(group, start, previous start, window slot, mirrored to disk)`.
pub(super) type ReservedSlot = (Group, u64, Option<u64>, bool, bool);

/// A reserved start and window slot that hands itself back unless kept: an
/// explicit failure awaits the release, and a dropped admission future (a
/// timeout wrapper, a `select!` that lost) releases it from `Drop`.
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
        if let Some((group, start, previous, window, persisted)) = self.slot.take() {
            self.state
                .release_reservation(group, start, previous, window, persisted)
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

impl Drop for Queued {
    fn drop(&mut self) {
        self.counts
            .counter(self.window)
            .fetch_sub(1, Ordering::SeqCst);
    }
}
