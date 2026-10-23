//! The selection plan: disjoint selected/related/excluded partitions, the
//! eligible ordinals the candidate budget left unjudged, and the pool its
//! candidate page judges from; sealed and compressed, it is the replay token.
use super::{Candidate, Outcome};
use crate::execution::SelectionLimits;
use crate::num::u32_of_usize;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octocode_contract_types::{
    Selection, SelectionFailure, SelectionScope, SelectionStatus, SelectionUnit,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::io::Read;

/// Version 9 records related (near-miss) ordinals apart from unknown ones.
pub(super) const VERSION: u8 = 9;

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
    pub(super) checksum: String,
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
    let mut bytes = Vec::new();
    // Failure details repeat across candidates; compression keeps their exact
    // evidence in replay tokens. Bound expansion of caller-supplied tokens.
    const MAX_PLAN_BYTES: u64 = 4 * 1024 * 1024;
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .take(MAX_PLAN_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_PLAN_BYTES {
        return None;
    }
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
            total: u32_of_usize(total),
            reason: reason.map(str::to_owned),
            failures: Vec::new(),
            unjudged: Vec::new(),
            pool: Vec::new(),
            opens: false,
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
            "unjudged":self.unjudged, "pool":self.pool, "opens":self.opens,
            "failures":self.failures,
        }))
    }

    /// Records deferred ordinals that can fit a fresh page's budget.
    pub(super) fn mark_unjudged(
        &mut self,
        candidates: &[Candidate],
        admitted: &[usize],
        limits: SelectionLimits,
    ) {
        let admitted = admitted.iter().copied().collect::<HashSet<_>>();
        let mut runs: Vec<[u32; 2]> = Vec::new();
        for (ordinal, candidate) in candidates.iter().enumerate() {
            if !candidate.fits_budget(limits)
                || admitted.contains(&ordinal)
                || !self.in_pool(ordinal)
            {
                continue;
            }
            let ordinal = u32_of_usize(ordinal);
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
        for failure in &self.failures {
            let Ok(ordinal) = u32::try_from(failure.candidate) else {
                return false;
            };
            if ordinal >= self.total || !self.in_pool(ordinal as usize) || !seen.insert(ordinal) {
                return false;
            }
        }
        let judged = self.judged();
        if self.opens {
            return !self.pool.is_empty()
                && seen.is_empty()
                && self.status == SelectionStatus::Partial;
        }
        let size = self.pool_size();
        match self.status {
            SelectionStatus::Complete => judged == size,
            SelectionStatus::Partial => judged < size,
            SelectionStatus::Unavailable => judged == 0,
        }
    }

    /// The ordinals decided: selected, related or excluded.
    fn judged(&self) -> usize {
        self.selected.len() + self.related.len() + self.excluded.len()
    }

    /// Completes the plan once every pooled ordinal is decided; otherwise
    /// names why the rest stay unknown: the candidate budget left some of
    /// the pool unadmitted (`omitted`), else the first failure.
    pub(super) fn settle(&mut self, admitted: usize, omitted: &str) {
        let size = self.pool_size();
        if self.judged() == size {
            self.status = SelectionStatus::Complete;
        } else {
            self.reason = Some(if admitted < size {
                omitted.into()
            } else {
                self.failures.first().map_or_else(
                    || "classificationUnknown".to_owned(),
                    |failure| failure.error_code.to_string(),
                )
            });
        }
    }

    pub(super) fn finish(mut self) -> Outcome {
        self.selected.sort_unstable();
        self.related.sort_unstable();
        self.excluded.sort_unstable();
        self.seal();
        let selected = self.selected.iter().map(|&i| i as usize).collect();
        let related = self.related.iter().map(|&i| i as usize).collect();
        let excluded = self.excluded.iter().map(|&i| i as usize).collect();
        let unjudged = self.unjudged_ordinals().map(|i| i as usize).collect();
        let size = i64::try_from(self.pool_size()).unwrap_or(i64::MAX);
        let receipt = Selection {
            status: self.status,
            scope: self.scope,
            unit: self.unit,
            selected_items: self.selected.len() as i64,
            excluded_items: self.excluded.len() as i64,
            related_items: std::num::NonZeroU64::new(self.related.len() as u64),
            unknown_items: size - self.judged() as i64,
            total_items: size,
            reason: self.reason.clone(),
            failures: self.failures.clone(),
            usage: None,
        };
        #[expect(
            clippy::expect_used,
            reason = "Plan has only serializable fields and compression writes to an in-memory Vec"
        )]
        let encode = |plan: &Plan| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            serde_json::to_writer(&mut encoder, plan).expect("selection plan is serializable");
            URL_SAFE_NO_PAD.encode(encoder.finish().expect("in-memory compression"))
        };
        let next_candidates = (!self.unjudged.is_empty()).then(|| {
            let mut opening = Plan::new(
                self.binding.clone(),
                self.total as usize,
                SelectionStatus::Partial,
                self.scope,
                self.unit,
                None,
            );
            opening.pool = self.unjudged.clone();
            opening.opens = true;
            opening.seal();
            encode(&opening)
        });
        Outcome {
            selected,
            related,
            excluded,
            unjudged,
            receipt,
            token: encode(&self),
            pool: self.pool,
            next_candidates,
        }
    }
}
