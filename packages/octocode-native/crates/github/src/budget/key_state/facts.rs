//! What one key learns and reports: observed rate-limit headers, exhausted buckets and cooldowns,
//! code-search room, and the git gate.
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
