//! Admission on one key: waiting out blocks, taking permits and spacing or window slots, and
//! releasing a reservation.
use super::{Admission, KeyFacts, KeyState};
use crate::budget::slots::{Queued, Reserved, ReservedSlot, SlotPlan, Subject};
use crate::budget::{ClientWaits, ExecutorConfig, Group, acquire, now_ms, waits};
use crate::{ProviderError, ProviderErrorKind};
use octocode_contract_types::ClientWaitKind;
use octocode_io::budget::pause_reporting;
use octocode_io::cancellation::Cancellation;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::OwnedSemaphorePermit;

impl KeyState {
    /// Pre-send gate: wait out a block that ends before the deadline, or
    /// fail fast without sending: an open circuit, a spent quota the client
    /// does not pace ([`super::Block::waited_out`]), or a reset or `retry-after`
    /// that falls past the deadline.
    pub(crate) async fn wait_unblocked(
        self: &Arc<Self>,
        resource: &str,
        config: &ExecutorConfig,
        deadline: Instant,
        cancellation: &Cancellation,
        client_waits: &ClientWaits,
    ) -> Result<(), ProviderError> {
        self.refresh_off_thread().await;
        for _ in 0..4 {
            let Some(block) = self.blocked(resource, config) else {
                return Ok(());
            };
            let now = now_ms();
            let wait = block.wait(now);
            if !block.waited_out() || wait >= deadline.saturating_duration_since(Instant::now()) {
                return Err(block.error(now));
            }
            client_waits
                .timed(
                    waits::block_kind(&block),
                    pause_reporting(wait, deadline, cancellation, |left| {
                        waits::blocked(&block, left)
                    }),
                )
                .await?;
        }
        match self.blocked(resource, config) {
            None => Ok(()),
            Some(block) => Err(block.error(now_ms())),
        }
    }

    /// Admit one attempt: group permit, group spacing (minTime, or the
    /// start the reported quota allows), then the per-key global permit. A limit that landed while this attempt queued
    /// is re-checked before it is sent: the reservation is handed back and
    /// the attempt waits the block out, or fails fast like the gate before it.
    pub(crate) async fn admit(
        self: &Arc<Self>,
        group: Option<Group>,
        config: &ExecutorConfig,
        deadline: Instant,
        cancellation: &Cancellation,
        client_waits: &ClientWaits,
    ) -> Result<Admission, ProviderError> {
        let resource = match group {
            Some(Group::Auth) => "auth",
            Some(group) => group.bucket(),
            None => "core",
        };
        for _ in 0..4 {
            if let Some(admission) = self
                .try_admit(
                    group,
                    resource,
                    config,
                    deadline,
                    cancellation,
                    client_waits,
                )
                .await?
            {
                return Ok(admission);
            }
            self.wait_unblocked(resource, config, deadline, cancellation, client_waits)
                .await?;
        }
        match self.blocked(resource, config) {
            None => Err(ProviderError::new(
                ProviderErrorKind::RateLimited,
                "GitHub request admission kept being blocked; request not sent.",
            )),
            Some(block) => Err(block.error(now_ms())),
        }
    }

    /// One admission pass; `None` when a block landed while it queued, after
    /// the reservation and permits were handed back.
    pub(in crate::budget) async fn try_admit(
        self: &Arc<Self>,
        group: Option<Group>,
        resource: &str,
        config: &ExecutorConfig,
        deadline: Instant,
        cancellation: &Cancellation,
        client_waits: &ClientWaits,
    ) -> Result<Option<Admission>, ProviderError> {
        let (reservation, mut permits) = match group {
            Some(group) => {
                self.reserve_group_slot(group, config, deadline, cancellation, client_waits)
                    .await?
            }
            None => (None, Vec::with_capacity(1)),
        };
        // Queued behind this key's other requests in flight: their starts
        // space this one's.
        let global = acquire(
            &self.global,
            deadline,
            cancellation,
            client_waits,
            ClientWaitKind::Spacing,
        )
        .await;
        let global = match global {
            Ok(permit) => permit,
            Err(error) => {
                if let Some(reserved) = reservation {
                    reserved.release().await;
                }
                return Err(error);
            }
        };
        // The sleeps above can span a response that set a cooldown or
        // exhausted the bucket: never send into a block learned meanwhile.
        self.refresh_off_thread().await;
        if self.blocked(resource, config).is_some() {
            if let Some(reserved) = reservation {
                reserved.release().await;
            }
            return Ok(None);
        }
        if let Some(reserved) = reservation {
            reserved.keep();
        }
        permits.push(global);
        Ok(Some(Admission { _permits: permits }))
    }

    /// The group half of an admission: the group permit, then the group's
    /// start slot, waited out before it returns. The reservation is handed back on an error here and, through
    /// `Drop`, when the admission future is dropped mid-wait.
    pub(in crate::budget) async fn reserve_group_slot(
        self: &Arc<Self>,
        group: Group,
        config: &ExecutorConfig,
        deadline: Instant,
        cancellation: &Cancellation,
        client_waits: &ClientWaits,
    ) -> Result<(Option<Reserved>, Vec<OwnedSemaphorePermit>), ProviderError> {
        // A group's start gap, fixed or derived from the reported quota, is
        // routine spacing: how every request of the group is sent.
        let kind = ClientWaitKind::Spacing;
        let mut permits = Vec::with_capacity(2);
        let plan = SlotPlan::new(group, config, 1, deadline);
        let persisted = matches!(group, Group::Search | Group::CodeSearch | Group::Graphql);
        let lane = self.groups.get(group.name());
        let mut queued = None;
        if plan.takes_slot() {
            // Peek before the group permit: a wait the current facts
            // already put past the deadline is refused now, not after queueing
            // behind another request's permit. The admissions queued
            // ahead in this process take their slots first, so the peek
            // plans past them. Entering the queue and peeking share one
            // facts lock, and an admission leaves the queue under the
            // lock that records its slot, so each one is counted exactly
            // once. The reserve below stays authoritative: a peer process
            // can take the slot meanwhile.
            if persisted {
                self.refresh_off_thread().await;
            }
            let facts = self.facts();
            let (entered, ahead) = lane.map(Queued::enter).unzip();
            queued = entered;
            let peeked = plan.start_after(&facts, now_ms(), ahead.unwrap_or_default());
            drop(facts);
            if let Some(refusal) = plan.refusal(peeked, Subject::Request) {
                return Err(refusal);
            }
        }
        if let Some(lane) = lane {
            permits.push(acquire(&lane.permit, deadline, cancellation, client_waits, kind).await?);
        }
        if !plan.takes_slot() {
            return Ok((None, permits));
        }
        // Take the start slot from this key's facts: the spacing after the
        // group's last start that the reported (or assumed) quota derives.
        // The deadline alone bounds the wait.
        let reserve = |facts: &mut KeyFacts| {
            drop(queued);
            let now = now_ms();
            let planned = plan.start(facts, now);
            if let Some(refusal) = plan.refusal(planned, Subject::Request) {
                return Err(refusal);
            }
            let start = planned.start;
            let previous = facts.last_start_ms.insert(group.name().to_owned(), start);
            Ok((start, previous))
        };
        // Mirrored groups reserve under the cross-process file lock, against
        // every process's slots: reserving from each process's own view lets
        // two processes take the same slot.
        let (start, previous) = if persisted {
            self.reserve_shared(deadline, cancellation, client_waits, reserve)
                .await??
        } else {
            reserve(&mut self.facts())?
        };
        let reserved = Reserved {
            state: Arc::clone(self),
            slot: Some((group, start, previous, persisted)),
        };
        if let Err(error) = client_waits
            .timed(
                kind,
                pause_reporting(
                    Duration::from_millis(start.saturating_sub(now_ms())),
                    deadline,
                    cancellation,
                    |left| waits::paced(group, left),
                ),
            )
            .await
        {
            // Nothing was sent: hand back the reserved start so it does not
            // throttle later requests.
            reserved.release().await;
            return Err(error.into());
        }
        Ok((Some(reserved), permits))
    }

    /// Hand back a reservation in memory and, when mirrored, on disk: the
    /// disk copy was written before the wait and a plain merge would restore it.
    pub(in crate::budget) async fn release_reservation(
        self: &Arc<Self>,
        group: Group,
        start: u64,
        previous: Option<u64>,
        persisted: bool,
    ) {
        self.facts().release(group.name(), start, previous);
        if persisted {
            // The persist takes the peer-shared file lock and may poll it: it
            // runs off the async worker, tracked until it ends.
            self.persist_awaited(move |view| view.release(group.name(), start, previous))
                .await;
        }
    }

    /// [`Self::release_reservation`] without awaiting: the in-memory slot is
    /// handed back at once and the disk copy is a detached write
    /// ([`Self::persist_detached`]).
    pub(in crate::budget) fn release_reservation_detached(
        self: &Arc<Self>,
        (group, start, previous, persisted): ReservedSlot,
    ) {
        self.facts().release(group.name(), start, previous);
        if persisted {
            self.persist_detached(move |view| view.release(group.name(), start, previous));
        }
    }

    /// `refresh_from_disk` for async callers: the stat and read run on a
    /// blocking thread, not on the executor worker that polls this request.
    pub(in crate::budget) async fn refresh_off_thread(self: &Arc<Self>) {
        let state = Arc::clone(self);
        let _ = tokio::task::spawn_blocking(octocode_io::notice::carried(move || {
            state.refresh_from_disk();
        }))
        .await;
    }
}
