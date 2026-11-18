//! A finished plan as its outcome: the receipt counting the whole walk, the
//! sealed replay token, and the token opening the next candidate page.
use super::{Plan, Tally};
use crate::selection::Outcome;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octocode_contract_types::{Selection, SelectionStatus};
use octocode_engine::num::saturating_u32;

impl Plan {
    /// Units of the walk this page and the pages before it decided, and the
    /// rest: undecided or left for a later candidate page.
    fn walk(&self) -> (u32, u32, u32, u32) {
        let decided = |page: usize, earlier: u32| saturating_u32(page).saturating_add(earlier);
        let selected = decided(self.selected.len(), self.carried.selected);
        let related = decided(self.related.len(), self.carried.related);
        let excluded = decided(self.excluded.len(), self.carried.excluded);
        let unknown = saturating_u32(self.pool_size().saturating_sub(self.judged()))
            .saturating_add(self.carried.unknown);
        (selected, related, excluded, unknown)
    }

    /// The ordinals no page of the walk decided and none will judge,
    /// ascending: the earlier pages' undecided ones and this pool's ordinals
    /// judged neither selected, related nor excluded (unknown answers, units
    /// without a state), less those left to `nextCandidatePage`. They are
    /// the units `unknownItems` counts, less the pending ones. Needs the
    /// judged lists sorted ([`Self::finish`]).
    fn undecided_ordinals(&self) -> Vec<u32> {
        let mut undecided = self
            .carried
            .undecided
            .iter()
            .flat_map(|[start, end]| *start..*end)
            .chain((0..self.total).filter(|&ordinal| {
                super::in_runs(&self.pool, ordinal)
                    && !(!self.unjudged.is_empty() && super::in_runs(&self.unjudged, ordinal))
                    && [&self.selected, &self.related, &self.excluded]
                        .iter()
                        .all(|judged| judged.binary_search(&ordinal).is_err())
            }))
            .collect::<Vec<_>>();
        undecided.sort_unstable();
        undecided
    }

    #[expect(
        clippy::expect_used,
        reason = "Plan has only serializable fields and compression writes to an in-memory Vec"
    )]
    fn encode(&self) -> String {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        serde_json::to_writer(&mut encoder, self).expect("selection plan is serializable");
        URL_SAFE_NO_PAD.encode(encoder.finish().expect("in-memory compression"))
    }

    /// The token replaying this finished page as its walk's undecided view.
    pub(in super::super) fn undecided_view(&self) -> String {
        let mut view = self.clone();
        view.undecided = true;
        view.seal();
        view.encode()
    }

    pub(in super::super) fn finish(mut self) -> Outcome {
        self.selected.sort_unstable();
        self.related.sort_unstable();
        self.excluded.sort_unstable();
        self.seal();
        let undecided = self.undecided_ordinals();
        let lists_undecided = self.undecided;
        let ordinals = |list: &[u32]| list.iter().map(|&i| i as usize).collect::<Vec<_>>();
        let (selected, related, excluded, unjudged) = if lists_undecided {
            (ordinals(&undecided), Vec::new(), Vec::new(), Vec::new())
        } else {
            (
                ordinals(&self.selected),
                ordinals(&self.related),
                ordinals(&self.excluded),
                self.unjudged_ordinals().map(|i| i as usize).collect(),
            )
        };
        // Every page of a chain counts the whole walk.
        let (selected_items, related_items, excluded_items, unknown_items) = self.walk();
        let receipt = Selection {
            status: self.status,
            scope: self.scope,
            unit: self.unit,
            selected_items: i64::from(selected_items),
            excluded_items: i64::from(excluded_items),
            related_items: std::num::NonZeroU64::new(u64::from(related_items)),
            unknown_items: i64::from(unknown_items),
            total_items: i64::from(selected_items)
                + i64::from(related_items)
                + i64::from(excluded_items)
                + i64::from(unknown_items),
            reason: self.reason.clone(),
            failures: self.failures.clone(),
            usage: None,
        };
        let next_candidates = (!lists_undecided && !self.unjudged.is_empty()).then(|| {
            let mut opening = Self::new(
                self.binding.clone(),
                self.total as usize,
                SelectionStatus::Partial,
                self.scope,
                self.unit,
                None,
            );
            opening.pool = self.unjudged.clone();
            opening.opens = true;
            let pending = saturating_u32(self.unjudged_ordinals().count());
            let settled = undecided.iter().map(|&ordinal| ordinal as usize);
            opening.carried = Tally {
                selected: selected_items,
                related: related_items,
                excluded: excluded_items,
                unknown: unknown_items.saturating_sub(pending),
                unanswered: self.carried.unanswered.saturating_add(self.unanswered),
                reason: self
                    .carried
                    .reason
                    .clone()
                    .or_else(|| (self.unanswered > 0).then(|| self.reason.clone()).flatten()),
                undecided: crate::line_spans::runs(settled)
                    .into_iter()
                    .map(|(start, end)| [saturating_u32(start), saturating_u32(end + 1)])
                    .collect(),
            };
            opening.seal();
            opening.encode()
        });
        Outcome {
            selected,
            related,
            excluded,
            unjudged,
            receipt,
            token: self.encode(),
            // A view's ordinals span the whole walk, earlier pages' too (no
            // runs: every ordinal).
            pool: if lists_undecided {
                Vec::new()
            } else {
                self.pool.clone()
            },
            next_candidates,
            undecided: ordinals(&undecided),
            plan: self,
            lists_undecided,
            long_context_notice: None,
        }
    }
}
