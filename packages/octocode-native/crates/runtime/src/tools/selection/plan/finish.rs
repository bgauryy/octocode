//! A finished plan as its outcome: the receipt counting the whole walk, the
//! sealed replay token, and the token opening the next candidate page.
use super::{Plan, Tally};
use crate::tools::selection::Outcome;
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

    pub(in super::super) fn finish(mut self) -> Outcome {
        self.selected.sort_unstable();
        self.related.sort_unstable();
        self.excluded.sort_unstable();
        self.seal();
        let selected = self.selected.iter().map(|&i| i as usize).collect();
        let related = self.related.iter().map(|&i| i as usize).collect();
        let excluded = self.excluded.iter().map(|&i| i as usize).collect();
        let unjudged = self.unjudged_ordinals().map(|i| i as usize).collect();
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
        #[expect(
            clippy::expect_used,
            reason = "Plan has only serializable fields and compression writes to an in-memory Vec"
        )]
        let encode = |plan: &Self| {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            serde_json::to_writer(&mut encoder, plan).expect("selection plan is serializable");
            URL_SAFE_NO_PAD.encode(encoder.finish().expect("in-memory compression"))
        };
        let next_candidates = (!self.unjudged.is_empty()).then(|| {
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
            };
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
