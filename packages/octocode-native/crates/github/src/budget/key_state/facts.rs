//! What one key learns and reports: observed rate-limit headers, exhausted buckets and cooldowns,
//! code-search quota room, and the git gate.
use super::{Bucket, KeyState};
use crate::ProviderError;
use crate::budget::slots::{SlotPlan, Subject};
use crate::budget::{ExecutorConfig, Group, now_ms};
use octocode_io::{budget::BudgetStop, retry::header_u64};
use reqwest::header::HeaderMap;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::OwnedSemaphorePermit;

impl KeyState {
    /// Record the primary bucket carried by any response. An exhausted
    /// bucket, and any bucket a quota-paced group spaces its starts by, is
    /// mirrored at once, so the next process blocks or paces from it too.
    pub(crate) fn observe(self: &Arc<Self>, headers: &HeaderMap, fallback_resource: &str) {
        let (Some(remaining), Some(reset)) = (
            header_u64(headers, "x-ratelimit-remaining"),
            header_u64(headers, "x-ratelimit-reset"),
        ) else {
            return;
        };
        let resource = crate::budget::rate_limit_resource(headers, fallback_resource);
        let paces = Group::paces_bucket(&resource);
        self.facts()
            .buckets
            .insert(resource, Bucket { remaining, reset });
        if remaining == 0 || paces {
            self.persist();
        }
    }

    /// A row that needs `searches` sends (one per extension) is refused
    /// before the first one when the planner puts the last start at or past
    /// the deadline; a refusal midway would spend quota on a page that is then
    /// discarded. The starts are simulated with the rule the admission uses,
    /// on a copy of the facts, so nothing is reserved.
    pub(crate) fn code_search_batch_refusal(
        &self,
        searches: usize,
        config: &ExecutorConfig,
        deadline: Instant,
    ) -> Option<ProviderError> {
        if searches < 2 {
            return None;
        }
        self.refresh_from_disk();
        let plan = SlotPlan::new(Group::CodeSearch, config, searches, deadline);
        let facts = self.facts();
        let ahead = self.groups.get(Group::CodeSearch.name()).map_or(0, |lane| {
            lane.queued.load(std::sync::atomic::Ordering::SeqCst)
        });
        let last = plan.batch_start(&facts, now_ms(), ahead);
        drop(facts);
        plan.refusal(last, Subject::Batch(searches))
    }

    /// Code searches the quota still covers: what GitHub last reported for
    /// the `code_search` resource (shared by every process on this key), or
    /// the assumed quota until a response reports one. An unpaced config
    /// plans against no quota.
    pub(crate) fn code_search_room(&self, config: &ExecutorConfig) -> usize {
        if !config.quota_pacing {
            return usize::MAX;
        }
        self.refresh_from_disk();
        self.facts()
            .quota(Group::CodeSearch, now_ms())
            .map_or(usize::MAX, |bucket| {
                usize::try_from(bucket.remaining).unwrap_or(usize::MAX)
            })
    }

    /// Record the `max-age` GitHub stated for a pull request (0: none), mirrored
    /// so the next process knows it too.
    pub(crate) fn learn_pull_request_max_age(self: &Arc<Self>, seconds: u64) {
        {
            let mut facts = self.facts();
            if facts
                .pull_request_max_age
                .is_some_and(|known| known <= seconds)
            {
                return;
            }
            facts.pull_request_max_age = Some(seconds);
        }
        self.persist();
    }

    /// How long GitHub says a pull request's REST read stands, once known and
    /// when it allows reuse.
    pub(crate) fn pull_request_lifetime(&self) -> Option<Duration> {
        self.facts()
            .pull_request_max_age
            .filter(|seconds| *seconds > 0)
            .map(Duration::from_secs)
    }

    /// The listing route found for `owner` on this key, by this process or,
    /// through the mirror, an earlier one.
    pub(crate) fn owner_route(&self, owner: &str) -> Option<String> {
        self.facts()
            .owner_routes
            .get(&owner.to_ascii_lowercase())
            .cloned()
    }

    /// Remember `owner`'s listing route, on disk before the request ends, so
    /// the next process skips the organization probe too.
    pub(crate) async fn learn_owner_route(self: &Arc<Self>, owner: &str, route: &str) {
        let known = self
            .facts()
            .owner_routes
            .insert(owner.to_ascii_lowercase(), route.to_owned());
        if known.as_deref() != Some(route) {
            self.persist_awaited(|_| {}).await;
        }
    }

    /// An owner that no longer answers its route (renamed, converted): gone
    /// from memory and from the mirror, unless a newer route replaced it there.
    pub(crate) async fn forget_owner_route(self: &Arc<Self>, owner: &str) {
        let owner = owner.to_ascii_lowercase();
        let Some(stale) = self.facts().owner_routes.remove(&owner) else {
            return;
        };
        self.persist_awaited(move |view| {
            if view.owner_routes.get(&owner) == Some(&stale) {
                view.owner_routes.remove(&owner);
            }
        })
        .await;
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

    /// Blocking `git` admission for clone subprocesses: waits out the key's
    /// secondary cooldown while it ends before `deadline`, as an API request
    /// does (an open circuit, or a cooldown past the deadline, fails at once),
    /// then takes a `git` group permit. The time spent is recorded on `waits`
    /// once, by cause, however the wait ends: a cooldown as `secondaryLimit`,
    /// the permit queue as `spacing`.
    pub(crate) fn acquire_git_blocking(
        &self,
        config: &ExecutorConfig,
        deadline: Instant,
        is_cancelled: &dyn Fn() -> bool,
        waits: &crate::budget::ClientWaits,
    ) -> Result<OwnedSemaphorePermit, ProviderError> {
        use octocode_contract_base::ClientWaitKind;
        let mut cooled = Duration::ZERO;
        let mut queued = Duration::ZERO;
        let ended = self.git_admission(config, deadline, is_cancelled, &mut cooled, &mut queued);
        waits.record(ClientWaitKind::SecondaryLimit, cooled);
        waits.record(ClientWaitKind::Spacing, queued);
        ended
    }

    /// [`Self::acquire_git_blocking`]'s loop, adding each sleep to its cause.
    fn git_admission(
        &self,
        config: &ExecutorConfig,
        deadline: Instant,
        is_cancelled: &dyn Fn() -> bool,
        cooled: &mut Duration,
        queued: &mut Duration,
    ) -> Result<OwnedSemaphorePermit, ProviderError> {
        const TICK: Duration = Duration::from_millis(25);
        let sleep = |pause: Duration, cause: &mut Duration| {
            let started = Instant::now();
            std::thread::sleep(pause);
            *cause += started.elapsed();
        };
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
                if block.circuit.is_some() || Instant::now() + wait >= deadline {
                    return Err(block.error(now));
                }
                sleep(wait.min(TICK), cooled);
                continue;
            }
            if let Ok(permit) = self.git.clone().try_acquire_owned() {
                return Ok(permit);
            }
            sleep(TICK, queued);
        }
    }
}
