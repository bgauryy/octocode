//! Per-key state: the blocking facts (buckets, cooldown, starts, circuit),
//! the lanes and permits, and every admission and release on them.
use super::slots::{Lane, Queued, Reserved, ReservedSlot, SlotPlan, Subject};
use super::{
    CircuitCause, ExecutorConfig, Group, LimiterKey, acquire, ceil_secs, now_ms, pause, persist,
    rate_limited_error,
};
use crate::{ProviderError, ProviderErrorKind, RateLimit};
use octocode_io::{budget::BudgetStop, retry::header_u64};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Bucket {
    pub remaining: u64,
    /// Epoch seconds.
    pub reset: u64,
}

/// Blocking facts for one key. Circuit fields are process-local.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct KeyFacts {
    #[serde(default)]
    pub(super) buckets: BTreeMap<String, Bucket>,
    #[serde(default)]
    pub(super) cooldown_until_ms: u64,
    #[serde(default)]
    pub(super) last_start_ms: BTreeMap<String, u64>,
    #[serde(default)]
    pub(super) code_search_ms: VecDeque<u64>,
    #[serde(skip)]
    pub(super) circuit_failures: u32,
    #[serde(skip)]
    pub(super) circuit_open_until_ms: u64,
    /// What the last counted failure was: the open circuit reports it.
    #[serde(skip)]
    pub(super) circuit_cause: Option<CircuitCause>,
}

impl KeyFacts {
    /// Undo one reservation made by `admit` (its start and window slot) when
    /// the request was never sent.
    pub(super) fn release(&mut self, group: &str, start: u64, previous: Option<u64>, window: bool) {
        if self.last_start_ms.get(group) == Some(&start) {
            match previous {
                Some(previous) => self.last_start_ms.insert(group.to_owned(), previous),
                None => self.last_start_ms.remove(group),
            };
        }
        if window
            && let Some(index) = self
                .code_search_ms
                .iter()
                .rposition(|stamp| *stamp == start)
        {
            self.code_search_ms.remove(index);
        }
    }
}

/// What currently blocks a request on a key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Block {
    pub until_ms: u64,
    pub resource: String,
    pub remaining: Option<u64>,
    pub reset: Option<u64>,
    /// Set when the block is an open circuit: the failure that opened it.
    pub circuit: Option<CircuitCause>,
}

impl Block {
    pub(crate) fn wait(&self, now_ms: u64) -> Duration {
        Duration::from_millis(self.until_ms.saturating_sub(now_ms))
    }

    pub(crate) fn error(&self, now_ms: u64) -> ProviderError {
        let retry_after_seconds = Some(ceil_secs(self.wait(now_ms)));
        if let Some(cause) = self
            .circuit
            .filter(|cause| *cause != CircuitCause::Secondary)
        {
            // An outage is not a rate limit: name the real cause, retryable
            // once the circuit closes.
            let (kind, message) = match cause {
                CircuitCause::Transport => (
                    ProviderErrorKind::Transport,
                    "GitHub is unavailable after repeated connection failures; request not sent until the circuit closes.",
                ),
                _ => (
                    ProviderErrorKind::Server,
                    "GitHub is unavailable after repeated server errors; request not sent until the circuit closes.",
                ),
            };
            let mut error = ProviderError::new(kind, message);
            error.rate_limit = Some(Box::new(RateLimit {
                remaining: None,
                reset_epoch_seconds: None,
                retry_after_seconds,
                resource: None,
            }));
            return error;
        }
        let message = if self.circuit.is_some() {
            "GitHub secondary rate limits repeated; requests are paused, wait and retry."
        } else if self.reset.is_some() {
            "GitHub API rate limit exhausted; request not sent until the limit resets."
        } else {
            "GitHub secondary rate limit cooldown is active; request not sent."
        };
        rate_limited_error(
            message,
            RateLimit {
                remaining: self.remaining,
                reset_epoch_seconds: self.reset,
                retry_after_seconds,
                resource: Some(self.resource.as_str().into()),
            },
        )
    }
}

/// Permits for one in-flight request; released on drop (before any backoff).
pub(crate) struct Admission {
    pub(super) _permits: Vec<OwnedSemaphorePermit>,
}

/// Keeps the host's OAuth throttling permits until the request finishes.
#[must_use]
pub struct AuthAdmission {
    pub(super) _admission: Admission,
}

pub(crate) struct KeyState {
    pub(super) key: LimiterKey,
    pub(super) global: Arc<Semaphore>,
    pub(super) groups: HashMap<&'static str, Lane>,
    pub(super) git: Arc<Semaphore>,
    pub(super) facts: Mutex<KeyFacts>,
    pub(super) persist: Mutex<Option<persist::Persist>>,
}

impl KeyState {
    pub(super) fn new(key: LimiterKey, config: &ExecutorConfig) -> Self {
        // Code search gets its own lane: GitHub meters it in a separate
        // bucket, and a code-search window wait must not stall repo/issue
        // search behind it.
        let groups = [
            Group::Search,
            Group::CodeSearch,
            Group::Graphql,
            Group::Write,
            Group::Auth,
        ]
        .into_iter()
        .map(|group| {
            let lane = Lane {
                permit: Arc::new(Semaphore::new(1)),
                queued: Arc::default(),
            };
            (group.name(), lane)
        })
        .collect();
        Self {
            key,
            global: Arc::new(Semaphore::new(config.global_concurrency.max(1))),
            groups,
            git: Arc::new(Semaphore::new(config.git_concurrency.max(1))),
            facts: Mutex::new(KeyFacts::default()),
            persist: Mutex::new(None),
        }
    }

    /// Nothing holds or throttles this key: no permit is out, no admission
    /// queued, and no cooldown, circuit, exhausted bucket, spacing or
    /// code-search window still constrains the next request. Dropping such a
    /// key and recreating it later admits exactly what keeping it would.
    pub(super) fn is_idle(&self, config: &ExecutorConfig, now: u64) -> bool {
        let permits_free = self.global.available_permits() == config.global_concurrency.max(1)
            && self.git.available_permits() == config.git_concurrency.max(1)
            && self.groups.values().all(|lane| {
                lane.permit.available_permits() == 1
                    && lane.queued.plain.load(Ordering::SeqCst) == 0
                    && lane.queued.windowed.load(Ordering::SeqCst) == 0
            });
        if !permits_free {
            return false;
        }
        let spacing = config
            .search_spacing
            .max(config.graphql_spacing)
            .max(config.write_spacing)
            .as_millis() as u64;
        let facts = self.facts();
        facts.cooldown_until_ms <= now
            && facts.circuit_failures == 0
            && facts.circuit_open_until_ms <= now
            && facts
                .buckets
                .values()
                .all(|bucket| bucket.remaining > 0 || config.reset_until_ms(bucket.reset) <= now)
            && facts
                .code_search_ms
                .iter()
                .all(|stamp| stamp.saturating_add(60_000) <= now)
            && facts
                .last_start_ms
                .values()
                .all(|start| start.saturating_add(spacing) <= now)
    }

    pub(super) fn facts(&self) -> std::sync::MutexGuard<'_, KeyFacts> {
        self.facts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Current blocker for `resource`, if any: open circuit, secondary
    /// cooldown, or an exhausted primary bucket (until reset + grace).
    pub(crate) fn blocked(&self, resource: &str, config: &ExecutorConfig) -> Option<Block> {
        let now = now_ms();
        let mut facts = self.facts();
        if facts.circuit_open_until_ms > now {
            return Some(Block {
                until_ms: facts.circuit_open_until_ms,
                resource: resource.to_owned(),
                remaining: None,
                reset: None,
                circuit: Some(facts.circuit_cause.unwrap_or(CircuitCause::Server)),
            });
        }
        if facts.circuit_open_until_ms != 0 {
            facts.circuit_open_until_ms = 0;
            facts.circuit_failures = 0;
        }
        let mut block: Option<Block> = None;
        if facts.cooldown_until_ms > now {
            block = Some(Block {
                until_ms: facts.cooldown_until_ms,
                resource: resource.to_owned(),
                remaining: None,
                reset: None,
                circuit: None,
            });
        }
        if let Some(bucket) = facts.buckets.get(resource)
            && bucket.remaining == 0
        {
            let until = config.reset_until_ms(bucket.reset);
            if until > now && block.as_ref().is_none_or(|b| b.until_ms < until) {
                block = Some(Block {
                    until_ms: until,
                    resource: resource.to_owned(),
                    remaining: Some(0),
                    reset: Some(bucket.reset),
                    circuit: None,
                });
            }
        }
        block
    }

    /// Pre-send gate: wait out a short block (≤ `cap` and inside the
    /// deadline) or fail fast without sending.
    pub(crate) async fn wait_unblocked(
        self: &Arc<Self>,
        resource: &str,
        config: &ExecutorConfig,
        cap: Duration,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), ProviderError> {
        self.refresh_off_thread().await;
        for _ in 0..4 {
            let Some(block) = self.blocked(resource, config) else {
                return Ok(());
            };
            let now = now_ms();
            let wait = block.wait(now);
            if block.circuit.is_some() || wait > cap || Instant::now() + wait >= deadline {
                return Err(block.error(now));
            }
            pause(wait, deadline, cancellation).await?;
        }
        match self.blocked(resource, config) {
            None => Ok(()),
            Some(block) => Err(block.error(now_ms())),
        }
    }

    /// Admit one attempt: group permit, group spacing (minTime), optional
    /// code-search window slot (counted once per logical request), then the
    /// per-key global permit. A limit that landed while this attempt queued
    /// is re-checked before it is sent: the reservation is handed back and
    /// the attempt waits the block out, or fails fast like the gate before it.
    pub(crate) async fn admit(
        self: &Arc<Self>,
        group: Option<Group>,
        config: &ExecutorConfig,
        count_code_search: bool,
        cap: Duration,
        deadline: Instant,
        cancellation: &CancellationToken,
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
                    count_code_search,
                    cap,
                    deadline,
                    cancellation,
                )
                .await?
            {
                return Ok(admission);
            }
            self.wait_unblocked(resource, config, cap, deadline, cancellation)
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
    pub(super) async fn try_admit(
        self: &Arc<Self>,
        group: Option<Group>,
        resource: &str,
        config: &ExecutorConfig,
        count_code_search: bool,
        cap: Duration,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Option<Admission>, ProviderError> {
        let (reservation, mut permits) = match group {
            Some(group) => {
                self.reserve_group_slot(
                    group,
                    config,
                    count_code_search,
                    cap,
                    deadline,
                    cancellation,
                )
                .await?
            }
            None => (None, Vec::with_capacity(1)),
        };
        let global = acquire(&self.global, deadline, cancellation).await;
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
    /// start slot (and code-search window slot), waited out before it
    /// returns. The reservation is handed back on an error here and, through
    /// `Drop`, when the admission future is dropped mid-wait.
    pub(super) async fn reserve_group_slot(
        self: &Arc<Self>,
        group: Group,
        config: &ExecutorConfig,
        count_code_search: bool,
        cap: Duration,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(Option<Reserved>, Vec<OwnedSemaphorePermit>), ProviderError> {
        let mut permits = Vec::with_capacity(2);
        let plan = SlotPlan::new(group, config, count_code_search, cap, deadline);
        let window = plan.window;
        let persisted = matches!(group, Group::Search | Group::CodeSearch | Group::Graphql);
        let lane = self.groups.get(group.name());
        let mut queued = None;
        if plan.takes_slot() {
            // Peek before the group permit: a wait the current facts
            // already put past the cap is refused now, not after queueing
            // behind another request's permit. The admissions queued
            // ahead in this process take their slots first, so the peek
            // plans past them. Entering the queue and peeking share one
            // facts lock, and an admission leaves the queue under the
            // lock that records its slot, so each one is counted exactly
            // once. The reserve below stays authoritative: a peer process
            // can fill the window meanwhile.
            if persisted {
                self.refresh_off_thread().await;
            }
            let facts = self.facts();
            let (entered, ahead) = lane.map(|lane| Queued::enter(lane, window)).unzip();
            queued = entered;
            let peeked = plan.start_after(&facts, now_ms(), ahead.unwrap_or_default());
            drop(facts);
            if let Some(refusal) = plan.refusal(peeked, Subject::Request) {
                return Err(refusal);
            }
        }
        if let Some(lane) = lane {
            permits.push(acquire(&lane.permit, deadline, cancellation).await?);
        }
        if !plan.takes_slot() {
            return Ok((None, permits));
        }
        // Take the start slot (and window slot) from this key's facts: the
        // spacing after the group's last start, and the window's oldest stamp
        // when the window is full. One cap, `cap`, bounds every wait, the
        // code-search window's too.
        let reserve = |facts: &mut KeyFacts| {
            drop(queued);
            let now = now_ms();
            if window {
                while facts
                    .code_search_ms
                    .front()
                    .is_some_and(|stamp| stamp.saturating_add(60_000) <= now)
                {
                    facts.code_search_ms.pop_front();
                }
            }
            let planned = plan.start(facts, now);
            if let Some(refusal) = plan.refusal(planned, Subject::Request) {
                return Err(refusal);
            }
            let start = planned.start;
            let previous = facts.last_start_ms.insert(group.name().to_owned(), start);
            if window {
                facts.code_search_ms.push_back(start);
            }
            Ok((start, previous))
        };
        // Mirrored groups reserve under the cross-process file lock, against
        // every process's slots: reserving from each process's own view lets
        // two processes take the same slot.
        let (start, previous) = if persisted {
            self.reserve_shared(deadline, cancellation, reserve)
                .await??
        } else {
            reserve(&mut self.facts())?
        };
        let reserved = Reserved {
            state: Arc::clone(self),
            slot: Some((group, start, previous, window, persisted)),
        };
        if let Err(error) = pause(
            Duration::from_millis(start.saturating_sub(now_ms())),
            deadline,
            cancellation,
        )
        .await
        {
            // Nothing was sent: hand back the reserved start and window slot
            // so they do not throttle later requests.
            reserved.release().await;
            return Err(error);
        }
        Ok((Some(reserved), permits))
    }

    /// Hand back a reservation in memory and, when mirrored, on disk: the
    /// disk copy was written before the wait and a plain merge would restore it.
    pub(super) async fn release_reservation(
        self: &Arc<Self>,
        group: Group,
        start: u64,
        previous: Option<u64>,
        window: bool,
        persisted: bool,
    ) {
        self.facts().release(group.name(), start, previous, window);
        if persisted {
            // The persist takes the peer-shared file lock and may poll it: keep
            // that wait off the async worker.
            let state = Arc::clone(self);
            let _ = tokio::task::spawn_blocking(octocode_io::notice::carried(move || {
                state.persist_with(|view| view.release(group.name(), start, previous, window));
            }))
            .await;
        }
    }

    /// [`Self::release_reservation`] without awaiting: the in-memory slot is
    /// handed back at once and the disk copy on the blocking pool.
    pub(super) fn release_reservation_detached(
        self: &Arc<Self>,
        (group, start, previous, window, persisted): ReservedSlot,
    ) {
        self.facts().release(group.name(), start, previous, window);
        if persisted {
            let state = Arc::clone(self);
            let write = octocode_io::notice::carried(move || {
                state.persist_with(|view| view.release(group.name(), start, previous, window))
            });
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => drop(handle.spawn_blocking(write)),
                Err(_) => write(),
            }
        }
    }

    /// `refresh_from_disk` for async callers: the stat and read run on a
    /// blocking thread, not on the executor worker that polls this request.
    pub(super) async fn refresh_off_thread(self: &Arc<Self>) {
        let state = Arc::clone(self);
        let _ = tokio::task::spawn_blocking(octocode_io::notice::carried(move || {
            state.refresh_from_disk()
        }))
        .await;
    }

    /// Record the primary bucket carried by any response.
    pub(crate) fn observe(self: &Arc<Self>, headers: &HeaderMap, fallback_resource: &str) {
        let (Some(remaining), Some(reset)) = (
            header_u64(headers, "x-ratelimit-remaining"),
            header_u64(headers, "x-ratelimit-reset"),
        ) else {
            return;
        };
        let resource = headers
            .get("x-ratelimit-resource")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_resource)
            .to_owned();
        self.facts()
            .buckets
            .insert(resource, Bucket { remaining, reset });
        if remaining == 0 {
            self.persist();
        }
    }

    /// A row that needs `searches` sends (one per extension) is refused
    /// before the first one when the planner puts the last start past the
    /// cap or the deadline; a refusal midway would spend window slots on a
    /// page that is then discarded. The starts are simulated with the rule
    /// the admission uses, on a copy of the facts, so nothing is reserved.
    pub(crate) fn code_search_batch_refusal(
        &self,
        searches: usize,
        config: &ExecutorConfig,
        cap: Duration,
        deadline: Instant,
    ) -> Option<ProviderError> {
        if searches < 2 {
            return None;
        }
        self.refresh_from_disk();
        let plan = SlotPlan::new(Group::CodeSearch, config, true, cap, deadline);
        let facts = self.facts();
        let mut ahead = self
            .groups
            .get(Group::CodeSearch.name())
            .map(|lane| lane.queued.ahead())
            .unwrap_or_default();
        ahead.add(plan.window, searches - 1);
        let last = plan.start_after(&facts, now_ms(), ahead);
        drop(facts);
        plan.refusal(last, Subject::Batch(searches))
    }

    /// Code searches the 60 s window still has room for, counting what other
    /// processes on this key reserved.
    pub(crate) fn code_search_room(&self, config: &ExecutorConfig) -> usize {
        self.refresh_from_disk();
        let now = now_ms();
        let used = self
            .facts()
            .code_search_ms
            .iter()
            .filter(|stamp| stamp.saturating_add(60_000) > now)
            .count();
        config.code_search_per_minute.saturating_sub(used)
    }

    /// Mark a resource exhausted until `reset` (epoch seconds).
    pub(crate) fn exhaust(self: &Arc<Self>, resource: &str, reset: u64) {
        self.facts().buckets.insert(
            resource.to_owned(),
            Bucket {
                remaining: 0,
                reset,
            },
        );
        self.persist();
    }

    /// Secondary limit: every caller of this key waits until `until_ms`.
    pub(crate) fn cool_down(self: &Arc<Self>, until_ms: u64) {
        {
            let mut facts = self.facts();
            facts.cooldown_until_ms = facts.cooldown_until_ms.max(until_ms);
        }
        self.persist();
    }

    /// Blocking `git` admission for clone subprocesses: honors the key's
    /// secondary cooldown / circuit, then takes a `git` group permit.
    pub(crate) fn acquire_git_blocking(
        &self,
        config: &ExecutorConfig,
        cap: Duration,
        deadline: Instant,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<OwnedSemaphorePermit, ProviderError> {
        const TICK: Duration = Duration::from_millis(25);
        self.refresh_from_disk();
        loop {
            if is_cancelled() {
                return Err(BudgetStop::Cancelled.into());
            }
            if Instant::now() >= deadline {
                return Err(BudgetStop::Deadline.into());
            }
            // Clone traffic is not metered by an API bucket; only the shared
            // cooldown/circuit apply.
            let block = self
                .blocked("git", config)
                .filter(|block| block.reset.is_none());
            if let Some(block) = block {
                let now = now_ms();
                let wait = block.wait(now);
                if block.circuit.is_some() || wait > cap || Instant::now() + wait >= deadline {
                    return Err(block.error(now));
                }
                std::thread::sleep(wait.min(TICK));
                continue;
            }
            if let Ok(permit) = self.git.clone().try_acquire_owned() {
                return Ok(permit);
            }
            std::thread::sleep(TICK);
        }
    }
}
