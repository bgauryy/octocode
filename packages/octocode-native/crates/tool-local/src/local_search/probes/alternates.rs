//! The other readings of a literal miss, offered as calls and never run:
//! case ignored, and the terms read as a regex. A reading would read every
//! file the page read once more; the caller decides whether it is worth that.

use crate::local_search::executor::Scanned;
use crate::local_search::local_search_query::{self, View};
use crate::local_search::types::{LocalSearchResult, SearchStatus};
use octocode_contract_types::{LocalSearchQuery, RegexEngine};
use serde_json::json;

/// The empty page's other readings of one request.
pub(in crate::local_search) struct Alternates<'q> {
    /// The request as sent, since a list runs joined and escaped.
    written: &'q LocalSearchQuery,
    /// How many matches or lines each term of the request held across the
    /// files the main scan kept, when the scan counted them per term.
    term_hits: Option<Vec<u64>>,
    /// The main scan covered its whole scope, a stream that walked it to its
    /// end included: its `term_hits` are totals, so a term at zero is held by
    /// no file.
    scope_counted: bool,
}

impl<'q> Alternates<'q> {
    pub(in crate::local_search) fn new(written: &'q LocalSearchQuery, scan: &Scanned) -> Self {
        let term_hits = scan
            .parsed
            .files
            .iter()
            .try_fold(None::<Vec<u64>>, |total, file| {
                let mut total = total.unwrap_or_else(|| vec![0; file.term_counts.len()]);
                if file.term_counts.len() != total.len() {
                    return None;
                }
                for (sum, count) in total.iter_mut().zip(&file.term_counts) {
                    *sum += u64::from(*count);
                }
                Some(Some(total))
            })
            .flatten();
        let stats = &scan.parsed.stats;
        // A stream counted its whole scope when its walk ran from the scope's
        // start to its end: every file it searched is in the page's scan.
        let whole_stream = scan.stream.as_ref().is_none_or(|facts| {
            facts.ended
                && facts.start_row == 0
                && facts.listed == 0
                && local_search_query::walk_offset(written).unwrap_or(0) == 0
        });
        let scope_counted = whole_stream
            && scan.seek.is_none()
            && scan.paused.is_none()
            && stats.cap_reasons.is_empty()
            && stats.capped != Some(true)
            && stats.error_count.unwrap_or(0) == 0;
        Self {
            written,
            term_hits,
            scope_counted,
        }
    }

    /// The hint of an empty search whose case rule may have decided it: a term
    /// is case-sensitive as written (`caseMode:"sensitive"`, or `smart` with an
    /// uppercase letter; a list runs joined as one case-sensitive alternation
    /// whatever its terms say). `None` for a page that lists files without the
    /// term, whose page is empty because the term is there.
    pub(in crate::local_search) fn case_hint(&self, query: &LocalSearchQuery) -> Option<String> {
        let written = self.written;
        (local_search_query::view(query) != View::FilesWithout
            && local_search_query::patterns(written)
                .0
                .iter()
                .any(|term| local_search_query::case_sensitive(written, term)))
        .then(|| {
            "No case-sensitive match; the related[] call with caseMode \"insensitive\" runs the same search with case ignored."
                .to_owned()
        })
    }

    /// Offer the regex reading of an empty literal search whose terms hold
    /// regex syntax: an agent writing `a|b` or `x[0-9]` usually means a regex,
    /// but `arr[i]` and `char*` are code, so the row stays the literal answer
    /// and the caller chooses.
    pub(in crate::local_search) fn offer_regex_reading(&self, row: &mut LocalSearchResult) {
        if row.status != SearchStatus::Empty || row.stats.files_searched == 0 {
            return;
        }
        let Some(reading) = self.regex_reading() else {
            return;
        };
        let lead = octocode_tools_shared::leads::skipped::lead(
            octocode_runtime_base::id::ToolId::LocalSearch,
            &reading,
            &[],
        );
        if let Some(map) = row.next.get_or_insert_with(|| json!({})).as_object_mut() {
            map.insert("asRegex".into(), lead);
        }
        row.hints.push(
            "No literal match; the terms hold regex syntax: the related[] call with regexEngine \"rust\" reads them as a regex."
                .to_owned(),
        );
    }

    /// Name each term of a several-term literal request that no file holds as
    /// written and that holds regex syntax, beside the terms that hit: the row
    /// is a success, and a dead term meant as a regex would otherwise stay
    /// silent. Only a scan that counted its whole scope settles a zero (it
    /// counts every term's own matches); a page it only partly read names
    /// nothing.
    pub(in crate::local_search) fn offer_dead_terms(&self, row: &mut LocalSearchResult) {
        use octocode_engine::search::text_pattern::regex_spells_itself;
        let written = self.written;
        let terms = &local_search_query::patterns(written).0;
        let view = local_search_query::view(written);
        // A files view stops each file at its first match, so its counts of
        // the other terms are floors.
        let Some(hits) = self
            .term_hits
            .as_ref()
            .filter(|_| self.scope_counted && view != View::Files)
        else {
            return;
        };
        if row.status == SearchStatus::Empty
            || terms.len() < 2
            || local_search_query::regex_mode(written) != RegexEngine::Literal
            || view == View::FilesWithout
            || local_search_query::invert_match(written) == Some(true)
            || local_search_query::snapshot(written).is_some()
            || local_search_query::page(written) > 1
        {
            return;
        }
        for (term, _) in terms.iter().zip(hits).filter(|(term, hits)| {
            **hits == 0 && !regex_spells_itself(term) && regex::Regex::new(term).is_ok()
        }) {
            row.warnings.push(format!(
                "No literal match for `{term}`, which holds regex syntax; regexEngine:\"rust\" reads every term as a regex."
            ));
        }
    }

    /// The written request read as a regex. `None` when no term holds regex
    /// syntax or a term fails to compile, on a shape a regex would not answer,
    /// or when the request cannot be restated.
    fn regex_reading(&self) -> Option<LocalSearchQuery> {
        let written = self.written;
        let terms = &local_search_query::patterns(written).0;
        if local_search_query::regex_mode(written) != RegexEngine::Literal
            || local_search_query::view(written) == View::FilesWithout
            || local_search_query::invert_match(written) == Some(true)
            || local_search_query::snapshot(written).is_some()
            || terms
                .iter()
                .all(|term| octocode_engine::search::text_pattern::regex_spells_itself(term))
            || terms.iter().any(|term| regex::Regex::new(term).is_err())
        {
            return None;
        }
        let mut fields = serde_json::to_value(written).ok()?;
        fields["regexEngine"] = json!(RegexEngine::Rust);
        serde_json::from_value::<LocalSearchQuery>(fields).ok()
    }
}
