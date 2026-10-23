//! What a selected page kept and removed: its receipt, token and counts.

use super::*;

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
    let saturate = |n: usize| u32_of_usize(n);
    let rows_are_occurrences = query.result_view == LocalSearchQueryResultView::MatchOnly
        && query.unique == LocalSearchQueryUnique::Off;
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

    /// The plan token later pages replay.
    pub(in crate::tools::local_search) fn token(&self) -> &str {
        &self.outcome.token
    }

    /// `next.unfiltered`, the whole unfiltered result of this scan, and
    /// `next.unfilteredUnknown` when it is narrower: the unknown hits (the
    /// same search over exactly the files holding them). Without unknown
    /// hits, or when they span every file, the whole result is the one
    /// lead. Groups the candidate budget left
    /// unjudged get `nextCandidatePage`: the same classified search of the
    /// same scan judging exactly them, so a chain of candidate pages judges
    /// every group once.
    pub(in crate::tools::local_search) fn leads(
        &self,
        query: &LocalSearchQuery,
        snapshot: &str,
        root_is_dir: bool,
    ) -> Vec<(&'static str, Value)> {
        // A page that kept every hit, or that the provider failed, already
        // shows every hit unfiltered.
        if !self.outcome.leaves_out() {
            return Vec::new();
        }
        let query_json = stated_query(query);
        let mut leads = self.unfiltered_leads(&query_json, snapshot, root_is_dir);
        if let Some(token) = self.outcome.next_candidates(ToolId::LocalSearch) {
            let mut row = query_json;
            if let Some(fields) = row.as_object_mut() {
                for field in crate::response::result::CURSOR_FIELDS {
                    fields.remove(field);
                }
            }
            row["snapshot"] = json!(snapshot);
            row["selection"] = json!(token);
            leads.push((
                "nextCandidatePage",
                crate::response::result::Continuation::new(ToolId::LocalSearch, row).build(),
            ));
        }
        leads
    }

    /// The whole unfiltered result of this scan.
    fn whole(&self, query_json: &Value, snapshot: &str) -> Value {
        let mut whole = crate::tools::selection::unfiltered_row(query_json, &[]);
        whole["snapshot"] = json!(snapshot);
        crate::response::result::Continuation::new(ToolId::LocalSearch, whole).build()
    }

    fn unfiltered_leads(
        &self,
        query_json: &Value,
        snapshot: &str,
        root_is_dir: bool,
    ) -> Vec<(&'static str, Value)> {
        let whole = self.whole(query_json, snapshot);
        let globs = self
            .unknown_files
            .iter()
            .map(|path| exact_glob(path))
            .collect::<Option<Vec<_>>>()
            .filter(|globs| {
                root_is_dir
                    && !globs.is_empty()
                    && globs.len() < self.scan_files
                    && globs.len() <= include_cap()
            });
        let Some(globs) = globs else {
            return vec![("unfiltered", whole)];
        };
        let mut unknown = crate::tools::selection::unfiltered_row(query_json, &[]);
        unknown["include"] = json!(globs);
        let unknown =
            crate::response::result::Continuation::new(ToolId::LocalSearch, unknown).build();
        vec![("unfiltered", whole), ("unfilteredUnknown", unknown)]
    }

    /// The provider's error when it failed this page, the hit rows the
    /// selected chunks hold of the pre-selection totals when selection
    /// removed hits (when it kept none, why: every chunk judged and none
    /// passed, which is the answer, or some could not be judged), and the
    /// groups the candidate budget left unjudged. That counts describe kept hits is the tool
    /// description's, not repeated here.
    pub(in crate::tools::local_search) fn disclosure(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(error) = &self.provider {
            notes.push(crate::tools::selection::unclassified_warning(
                error,
                "this search",
                "its hits are",
            ));
        }
        if self.rows_kept == 0 && self.rows_before > 0 && self.provider.is_none() {
            let unknown = self.outcome.receipt.unknown_items;
            let why = if unknown == 0 {
                "every chunk was judged and none passed the questions".to_owned()
            } else {
                format!("no chunk passed the questions and {unknown} could not be judged")
            };
            let answer = if unknown == 0 {
                "; that is the answer"
            } else {
                ""
            };
            notes.push(format!(
                "clasifyContext: {why}, so none of the {} hit rows ({} files) is listed{answer}.",
                self.rows_before, self.files_before,
            ));
        } else if self.rows_kept < self.rows_before {
            notes.push(format!(
                "clasifyContext: the selected chunks hold {} of {} hit rows ({} of {} files).",
                self.rows_kept, self.rows_before, self.files_kept, self.files_before,
            ));
        }
        notes
    }
}
