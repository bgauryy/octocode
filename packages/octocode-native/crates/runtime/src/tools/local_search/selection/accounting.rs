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
    ) -> Result<Vec<(&'static str, Value)>, ToolError> {
        // A page that kept every hit, or that the provider failed, already
        // shows every hit unfiltered.
        if !self.outcome.leaves_out() {
            return Ok(Vec::new());
        }
        let mut leads = self.unfiltered_leads(query, snapshot, root_is_dir)?;
        if let Some(token) = self.outcome.next_candidates(ToolId::LocalSearch) {
            let mut changes = serde_json::Map::new();
            for field in crate::contracts::state_fields(ToolId::LocalSearch) {
                changes.insert(field.clone(), Value::Null);
            }
            changes.insert("snapshot".into(), json!(snapshot));
            changes.insert("selection".into(), json!(token));
            let next = local_search_query::edited(query, &Value::Object(changes))?;
            leads.push((
                "nextCandidatePage",
                crate::response::result::Continuation::continued_query(next).build(),
            ));
        }
        Ok(leads)
    }

    fn unfiltered_leads(
        &self,
        query: &LocalSearchQuery,
        snapshot: &str,
        root_is_dir: bool,
    ) -> Result<Vec<(&'static str, Value)>, ToolError> {
        let mut changes = crate::tools::selection::unfiltered_changes(&[]);
        changes["snapshot"] = json!(snapshot);
        let whole = crate::response::result::Continuation::continued_query(
            local_search_query::edited(query, &changes)?,
        )
        .build();
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
            return Ok(vec![("unfiltered", whole)]);
        };
        let mut changes = crate::tools::selection::unfiltered_changes(&[]);
        changes["include"] = json!(globs);
        let unknown = crate::response::result::Continuation::continued_query(
            local_search_query::edited(query, &changes)?,
        )
        .build();
        Ok(vec![("unfiltered", whole), ("unfilteredUnknown", unknown)])
    }

    /// The provider's error when it failed this page, the hit rows kept
    /// (those that pass the classifiers) of the pre-selection totals when
    /// selection removed hits, and, when it kept none, why (every chunk
    /// judged and none passed, or some could not be judged) and that an
    /// empty classified result is no absence: the unfiltered search matched
    /// those rows and `next.unfiltered` lists them. That counts describe
    /// kept hits is the tool description's, not repeated here.
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
            notes.push(format!(
                "classification: {why}, so none of the {} ({}) is listed. An empty classified result is not absence: the unfiltered search matched them, and `next.unfiltered` lists them.",
                octocode_engine::text::counted(self.rows_before, "hit row", "hit rows"),
                octocode_engine::text::counted(self.files_before, "file", "files"),
            ));
        } else if self.rows_kept < self.rows_before {
            notes.push(format!(
                "classification kept {} of {} ({} of {}) that pass the classifiers.",
                self.rows_kept,
                octocode_engine::text::counted(self.rows_before, "hit row", "hit rows"),
                self.files_kept,
                octocode_engine::text::counted(self.files_before, "file", "files"),
            ));
        }
        notes
    }
}
