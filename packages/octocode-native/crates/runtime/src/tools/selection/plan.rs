//! The selection plan: disjoint selected/related/excluded partitions, the
//! eligible ordinals the candidate budget left unjudged, and the pool its
//! candidate page judges from; sealed and compressed, it is the replay token.
use super::Candidate;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octocode_contract_types::{SelectionFailure, SelectionScope, SelectionStatus, SelectionUnit};
use octocode_engine::num::saturating_u32;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

mod finish;

/// Version 12 carries the earlier candidate pages' counts and undecided
/// ordinals, so every page of a chain reports, and its undecided view lists,
/// the whole walk.
pub(super) const VERSION: u8 = 12;

/// What the candidate pages before this one decided or left unknown: their
/// share of the walk, so each page's receipt counts the whole of it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct Tally {
    pub(super) selected: u32,
    pub(super) related: u32,
    pub(super) excluded: u32,
    /// Undecided for good: answered unsure, failed, or never judgeable.
    pub(super) unknown: u32,
    /// Within `unknown`: never answered (a failure or no state to judge).
    pub(super) unanswered: u32,
    /// Why the first earlier page left units unanswered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) reason: Option<String>,
    /// The `unknown` ordinals themselves, as runs: the undecided view of a
    /// later page lists them beside its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "runs")]
    pub(super) undecided: Vec<[u32; 2]>,
}

impl Tally {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn items(&self) -> usize {
        [self.selected, self.related, self.excluded, self.unknown]
            .iter()
            .map(|&count| count as usize)
            .sum()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Plan {
    pub(super) version: u8,
    pub(super) binding: String,
    pub(super) status: SelectionStatus,
    pub(super) scope: SelectionScope,
    pub(super) unit: SelectionUnit,
    pub(super) selected: Vec<u32>,
    /// Judged related but not direct evidence: hidden, yet decided.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) related: Vec<u32>,
    pub(super) excluded: Vec<u32>,
    pub(super) total: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) failures: Vec<SelectionFailure>,
    /// Eligible ordinals the candidate budget left unjudged, as half-open
    /// `[start, end)` runs in source order, independent of admission order.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "runs")]
    pub(super) unjudged: Vec<[u32; 2]>,
    /// The ordinals this candidate page judges from, as runs; empty is every
    /// ordinal (the first candidate page). A later candidate page's pool is
    /// the unjudged rest of the page before it, so a chain of candidate
    /// pages judges each ordinal once.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "runs")]
    pub(super) pool: Vec<[u32; 2]>,
    /// The token opens the candidate page over `pool` (nothing judged yet)
    /// instead of replaying a judged page.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) opens: bool,
    /// The token replays this page as its undecided view: the pool's
    /// ordinals it judged neither selected, related nor excluded (unknown
    /// answers, unjudged units, units without a state), with no provider call.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) undecided: bool,
    /// The earlier candidate pages of this chain.
    #[serde(default, skip_serializing_if = "Tally::is_empty")]
    pub(super) carried: Tally,
    /// This page's pooled ordinals that got no answer: failed for good, or
    /// with no state to judge.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(super) unanswered: u32,
    pub(super) checksum: String,
}

fn is_zero(count: &u32) -> bool {
    *count == 0
}

/// Half-open ordinal runs as one flat list of gaps and lengths (each run's
/// start minus the previous run's end, then its length), so a plan stays
/// small however scattered its runs are. Decoded runs are ascending,
/// disjoint and non-empty.
mod runs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    pub(super) fn serialize<S: Serializer>(
        runs: &[[u32; 2]],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut flat = Vec::with_capacity(runs.len() * 2);
        let mut end = 0u32;
        for &[start, stop] in runs {
            flat.push(start.saturating_sub(end));
            flat.push(stop.saturating_sub(start));
            end = stop;
        }
        flat.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<[u32; 2]>, D::Error> {
        let flat = Vec::<u32>::deserialize(deserializer)?;
        if flat.len() % 2 != 0 {
            return Err(D::Error::custom("runs are gap and length pairs"));
        }
        let mut runs = Vec::with_capacity(flat.len() / 2);
        let mut end = 0u32;
        for &[gap, length] in flat.as_chunks::<2>().0 {
            let start = end
                .checked_add(gap)
                .ok_or_else(|| D::Error::custom("run past u32"))?;
            let stop = start
                .checked_add(length)
                .filter(|stop| *stop > start)
                .ok_or_else(|| D::Error::custom("empty run"))?;
            runs.push([start, stop]);
            end = stop;
        }
        Ok(runs)
    }
}

/// Whether `ordinal` lies in `runs` (empty runs hold every ordinal).
pub(super) fn in_runs(runs: &[[u32; 2]], ordinal: u32) -> bool {
    if runs.is_empty() {
        return true;
    }
    let at = runs.partition_point(|&[_, end]| end <= ordinal);
    runs.get(at).is_some_and(|&[start, _]| start <= ordinal)
}

pub(super) fn decode(token: &str) -> Option<Plan> {
    let compressed = URL_SAFE_NO_PAD.decode(token).ok()?;
    // Failure details repeat across candidates; compression keeps their exact
    // evidence in replay tokens. Bound expansion of caller-supplied tokens.
    const MAX_PLAN_BYTES: u64 = 4 * 1024 * 1024;
    let bytes = octocode_io::read::read_limited(
        flate2::read::ZlibDecoder::new(compressed.as_slice()),
        MAX_PLAN_BYTES,
    )
    .ok()??;
    serde_json::from_slice(&bytes).ok()
}

impl Plan {
    pub(super) fn new(
        binding: String,
        total: usize,
        status: SelectionStatus,
        scope: SelectionScope,
        unit: SelectionUnit,
        reason: Option<&str>,
    ) -> Self {
        Self {
            version: VERSION,
            binding,
            status,
            scope,
            unit,
            selected: Vec::new(),
            related: Vec::new(),
            excluded: Vec::new(),
            total: saturating_u32(total),
            reason: reason.map(str::to_owned),
            failures: Vec::new(),
            unjudged: Vec::new(),
            pool: Vec::new(),
            opens: false,
            undecided: false,
            carried: Tally::default(),
            unanswered: 0,
            checksum: String::new(),
        }
    }

    /// The ordinals this candidate page judges from.
    pub(super) fn pool_size(&self) -> usize {
        if self.pool.is_empty() {
            return self.total as usize;
        }
        self.pool
            .iter()
            .map(|[start, end]| (end - start) as usize)
            .sum()
    }

    pub(super) fn in_pool(&self, ordinal: usize) -> bool {
        u32::try_from(ordinal).is_ok_and(|ordinal| in_runs(&self.pool, ordinal))
    }

    pub(super) fn digest(&self) -> String {
        crate::digest::json_sha256(&json!({
            "version":self.version, "binding":self.binding, "status":self.status,
            "scope":self.scope, "unit":self.unit, "selected":self.selected,
            "related":self.related, "excluded":self.excluded, "total":self.total, "reason":self.reason,
            "unjudged":self.unjudged, "pool":self.pool, "opens":self.opens, "undecided":self.undecided,
            "failures":self.failures, "carried":self.carried, "unanswered":self.unanswered,
        }))
    }

    /// Records the ordinals this page's budget deferred.
    pub(super) fn mark_unjudged(&mut self, candidates: &[Candidate], admitted: &[usize]) {
        let admitted = admitted.iter().copied().collect::<HashSet<_>>();
        self.set_unjudged(
            candidates
                .iter()
                .enumerate()
                .filter(|(ordinal, candidate)| {
                    candidate.state.is_some()
                        && !admitted.contains(ordinal)
                        && self.in_pool(*ordinal)
                })
                .map(|(ordinal, _)| saturating_u32(ordinal))
                .collect(),
        );
    }

    /// Adds ordinals a later candidate page should judge again.
    pub(super) fn retry_later(&mut self, ordinals: impl Iterator<Item = u32>) {
        let mut all = self.unjudged_ordinals().collect::<Vec<_>>();
        all.extend(ordinals);
        all.sort_unstable();
        all.dedup();
        self.set_unjudged(all);
    }

    /// `ordinals` (ascending) as runs.
    fn set_unjudged(&mut self, ordinals: Vec<u32>) {
        let mut runs: Vec<[u32; 2]> = Vec::new();
        for ordinal in ordinals {
            match runs.last_mut() {
                Some(run) if run[1] == ordinal => run[1] = ordinal.saturating_add(1),
                _ => runs.push([ordinal, ordinal.saturating_add(1)]),
            }
        }
        self.unjudged = runs;
    }

    pub(super) fn unjudged_ordinals(&self) -> impl Iterator<Item = u32> + '_ {
        self.unjudged.iter().flat_map(|[start, end]| *start..*end)
    }

    pub(super) fn seal(&mut self) {
        self.checksum = self.digest();
    }

    pub(super) fn valid(
        &self,
        binding: &str,
        total: usize,
        scope: SelectionScope,
        unit: SelectionUnit,
    ) -> bool {
        if self.version != VERSION
            || (self.undecided && self.opens)
            || self.binding != binding
            || self.scope != scope
            || self.unit != unit
            || usize::try_from(self.total).ok() != Some(total)
            || self.judged() > total
            || self.checksum != self.digest()
        {
            return false;
        }
        if self.unjudged.iter().any(|[start, end]| start >= end)
            || self
                .pool
                .last()
                .is_some_and(|&[_, end]| usize::try_from(end).map_or(true, |end| end > total))
            // The earlier pages and this pool are the whole walk.
            || self.carried.items().checked_add(self.pool_size()) != Some(total)
            || self.carried.unanswered > self.carried.unknown
            // The earlier pages' undecided units lie outside this pool, and
            // are every unit they counted unknown.
            || self.carried.undecided.iter().any(|&[start, end]| {
                start >= end
                    || end > self.total
                    || (start..end).any(|ordinal| self.in_pool(ordinal as usize))
            })
            || self
                .carried
                .undecided
                .iter()
                .map(|[start, end]| u64::from(end - start))
                .sum::<u64>()
                != u64::from(self.carried.unknown)
        {
            return false;
        }
        let mut seen = HashSet::with_capacity(self.judged());
        if !self
            .selected
            .iter()
            .chain(&self.related)
            .chain(&self.excluded)
            .copied()
            .chain(self.unjudged_ordinals())
            .all(|ordinal| {
                usize::try_from(ordinal).is_ok_and(|i| i < total && self.in_pool(i))
                    && seen.insert(ordinal)
            })
        {
            return false;
        }
        // A failed ordinal is undecided; it may be unjudged, for a later page.
        let unjudged = self.unjudged_ordinals().collect::<HashSet<_>>();
        let mut failed = HashSet::with_capacity(self.failures.len());
        for failure in &self.failures {
            let Ok(ordinal) = u32::try_from(failure.candidate) else {
                return false;
            };
            if ordinal >= self.total
                || !self.in_pool(ordinal as usize)
                || !failed.insert(ordinal)
                || (seen.contains(&ordinal) && !unjudged.contains(&ordinal))
            {
                return false;
            }
        }
        let judged = self.judged();
        if self.opens {
            return !self.pool.is_empty()
                && seen.is_empty()
                && self.status == SelectionStatus::Partial;
        }
        match self.status {
            SelectionStatus::Complete => {
                judged <= self.pool_size() && self.unjudged.is_empty() && self.answered_all()
            }
            SelectionStatus::Partial => {
                judged < self.pool_size() || !self.unjudged.is_empty() || !self.answered_all()
            }
            SelectionStatus::Unavailable => judged == 0,
        }
    }

    /// Whether every unit of the walk outside this page's unjudged ones got
    /// an answer, an unsure one included.
    fn answered_all(&self) -> bool {
        self.unanswered == 0 && self.failures.is_empty() && self.carried.unanswered == 0
    }

    /// The ordinals decided: selected, related or excluded.
    fn judged(&self) -> usize {
        self.selected.len() + self.related.len() + self.excluded.len()
    }

    /// Completes the plan once every unit of the walk was judged, an unsure
    /// answer included, and none is left for a later candidate page; else
    /// it stays partial. Names why units stay unknown: the candidate budget
    /// left some of the pool unadmitted (`omitted`), else the first failure,
    /// else the judge's unsure answers, else an earlier page's reason.
    pub(super) fn settle(&mut self, admitted: usize, omitted: &str) {
        let size = self.pool_size();
        let pending = self.unjudged_ordinals().count();
        let retried = self.failures.len()
            - self
                .failures
                .iter()
                .filter(|failure| {
                    u32::try_from(failure.candidate)
                        .is_ok_and(|ordinal| !in_runs(&self.unjudged, ordinal))
                })
                .count();
        // Pooled ordinals never admitted nor left for later had no state to
        // judge; failures not retried got no answer either.
        let unadmitted = size.saturating_sub(admitted);
        let deferred = pending.saturating_sub(retried);
        self.unanswered =
            saturating_u32(unadmitted.saturating_sub(deferred) + (self.failures.len() - retried));
        self.status = if pending == 0 && self.answered_all() {
            SelectionStatus::Complete
        } else {
            SelectionStatus::Partial
        };
        self.reason = if self.judged() < size {
            Some(if admitted < size {
                omitted.into()
            } else {
                self.failures.first().map_or_else(
                    || "classificationUnknown".to_owned(),
                    |failure| failure.error_code.to_string(),
                )
            })
        } else {
            self.carried.reason.clone()
        };
    }
}
