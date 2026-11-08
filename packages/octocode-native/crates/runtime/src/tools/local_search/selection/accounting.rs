//! What a selected page kept and removed: its receipt, token and counts.
use crate::tools::local_search::local_search_query;
use crate::tools::local_search::local_search_query::Unique;
use crate::tools::local_search::local_search_query::View;

use super::*;
use octocode_contract_types::LocalSearchQuery;

pub(super) fn distinct_lines(file: &TextSearchFile) -> usize {
    file.matches
        .iter()
        .map(|hit| hit.line)
        .collect::<HashSet<_>>()
        .len()
}

/// Totals that describe the kept hits: matched lines lose the lines whose
/// every row was removed. Occurrences follow only when each row is one
/// occurrence (matchOnly without `unique`) or every hit line of the scan
/// holds exactly one; otherwise they are unknown (absent), never estimated.
pub(super) fn recount(
    query: &LocalSearchQuery,
    stats: &mut octocode_engine::types::TextSearchStats,
    rows_removed: usize,
    lines_removed: usize,
) {
    if rows_removed == 0 {
        return;
    }
    let saturate = |n: usize| saturating_u32(n);
    let rows_are_occurrences = local_search_query::view(query) == View::MatchOnly
        && local_search_query::unique(query) == Unique::Off;
    let one_per_line = stats.match_count.is_some() && stats.match_count == stats.matched_lines;
    stats.match_count = if rows_are_occurrences {
        stats
            .match_count
            .map(|n| n.saturating_sub(saturate(rows_removed)))
    } else if one_per_line {
        stats
            .match_count
            .map(|n| n.saturating_sub(saturate(lines_removed)))
    } else {
        None
    };
    stats.matched_lines = stats
        .matched_lines
        .map(|n| n.saturating_sub(saturate(lines_removed)));
}

impl Selected {
    /// The receipt the row reports.
    pub(in crate::tools::local_search) fn receipt(&self) -> &octocode_contract_types::Selection {
        &self.outcome.receipt
    }

    /// The receipt the row carries: none for an undecided view, an
    /// unfiltered listing as `next.unfiltered` is (its hits are not selected
    /// evidence; the page that decided them carries the receipt).
    pub(in crate::tools::local_search) fn into_receipt(
        self,
    ) -> Option<octocode_contract_types::Selection> {
        (!self.outcome.lists_undecided()).then_some(self.outcome.receipt)
    }

    /// The plan token later pages replay.
    pub(in crate::tools::local_search) fn token(&self) -> &str {
        &self.outcome.token
    }

    /// `next.unfiltered`, the whole unfiltered result of this scan, and
    /// `next.unfilteredUnknown` when it is narrower: exactly the hits this
    /// page left undecided (the same scan replaying the selection's
    /// undecided view, unbilled, paged as any result). Without undecided
    /// hits, or when every hit is one, the whole result is the one lead. An
    /// undecided view leads nowhere: it is that list. Hits the page's budget or a provider
    /// that could not answer left unjudged get `nextCandidatePage`: the same
    /// classified search of the same scan judging exactly them, so a chain of
    /// candidate pages judges every hit once.
    pub(in crate::tools::local_search) fn leads(
        &self,
        query: &LocalSearchQuery,
        snapshot: &str,
    ) -> Result<Vec<(&'static str, Value)>, ToolError> {
        // A page that kept every hit, or that the provider failed, already
        // shows every hit unfiltered.
        if !self.outcome.leaves_out() || self.outcome.lists_undecided() {
            return Ok(Vec::new());
        }
        let mut changes = crate::tools::selection::unfiltered_changes(&[]);
        changes["snapshot"] = json!(snapshot);
        let whole = crate::response::result::Continuation::continued_query(
            local_search_query::edited(query, &changes)?,
        )
        .build();
        let mut leads = vec![("unfiltered", whole)];
        let replays = [
            ("unfilteredUnknown", self.outcome.undecided_view()),
            (
                "nextCandidatePage",
                self.outcome.next_candidates().map(str::to_owned),
            ),
        ];
        for (channel, token) in replays {
            if let Some(token) = token {
                leads.push((channel, replay(query, snapshot, &token)?));
            }
        }
        Ok(leads)
    }

    /// The provider's error when it failed this page; else, when selection
    /// removed hits, what it kept over the whole walk
    /// ([`crate::tools::selection::kept_note`]).
    pub(in crate::tools::local_search) fn disclosure(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(error) = &self.provider {
            notes.push(crate::tools::selection::unclassified_notice(
                error,
                crate::tools::selection::Unclassified::Search,
            ));
        }
        if self.outcome.lists_undecided() {
            notes.push(crate::tools::selection::undecided_note(self.rows_kept));
            return notes;
        }
        if self.provider.is_none() && self.rows_before > 0 {
            notes.extend(crate::tools::selection::kept_note(
                &self.outcome,
                self.rows_kept,
            ));
        }
        notes.extend(self.outcome.long_context_notice.clone());
        notes
    }
}

/// The same classified search of the same scan replaying `token`, a
/// selection plan; the page fields start over.
fn replay(query: &LocalSearchQuery, snapshot: &str, token: &str) -> Result<Value, ToolError> {
    replay_call(crate::response::result::query_value(query), snapshot, token)
}

/// [`replay`] over the query row: a row that is no object has no
/// continuation, which is the caller's error, never a dropped lead.
pub(super) fn replay_call(query: Value, snapshot: &str, token: &str) -> Result<Value, ToolError> {
    crate::response::paging::next_candidate_page_call(
        ToolId::LocalSearch,
        query,
        token,
        Some(snapshot),
    )
    .ok_or_else(|| {
        ToolError::new(
            octocode_contract_types::ErrorCode::ExecutionFailed,
            "the continuation is no valid localSearch query: the query row is not an object",
        )
    })
}
