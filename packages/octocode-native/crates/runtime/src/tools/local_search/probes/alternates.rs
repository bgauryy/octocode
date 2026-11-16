//! The other readings of an empty search: with case ignored, and a literal
//! read as a regex. One pass over the files the request's own walk searched
//! runs them, so neither pays for a listing and admission of its own, and each
//! file is read once; a walk whose record cannot serve runs them as its own
//! probes. A probe says how many files, never what they hold.

use super::{probe_walked, query_walk, within_work};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::tools::local_search::executor::Scanned;
use crate::tools::local_search::local_search_query::{self, View};
use crate::tools::local_search::types::{LocalSearchResult, SearchStatus};
use octocode_contract_types::{LocalSearchQuery, RegexEngine};
use octocode_engine::search::text_search::{count_listed, listable};
use octocode_engine::types::{CaseRule, SearchOutput, TextSearchOptions};
use serde_json::json;
use std::cell::{Cell, OnceCell};
use std::path::PathBuf;

/// How many files a reading found; a floor when the probe budget cut it.
#[derive(Clone, Copy, Debug)]
struct Files {
    count: usize,
    cut: bool,
}

impl std::fmt::Display for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.cut {
            f.write_str("at least ")?;
        }
        write!(f, "{}", self.count)
    }
}

/// What the pass of [`Alternates::case_hint`] left for the regex reading.
#[derive(Clone, Copy)]
enum Pass {
    /// No pass ran, or it did not cover the regex reading.
    Unrun,
    /// The pass covered the reading and the probe found nothing to count.
    Empty,
    /// The pass covered the reading and found these files.
    Found(Files),
}

/// The request as a regex, and the search that reads it so.
struct Reading {
    query: LocalSearchQuery,
    probe: TextSearchOptions,
}

/// The empty page's other readings of one request.
pub(in crate::tools::local_search) struct Alternates<'q> {
    /// The request as sent, since a list runs joined and escaped.
    written: &'q LocalSearchQuery,
    /// What an empty scan's walk searched, when its record can serve.
    pub(in crate::tools::local_search) listed: Option<Vec<PathBuf>>,
    reading: OnceCell<Option<Reading>>,
    /// How many matches or lines each term of the request held across the
    /// files the main scan kept, when the scan counted them per term.
    term_hits: Option<Vec<u64>>,
    /// The main scan covered its whole scope: its `term_hits` are totals, so
    /// a term at zero is held by no file.
    scope_counted: bool,
    /// What the pass of [`Self::case_hint`] found for the regex reading.
    read: Cell<Pass>,
}

impl<'q> Alternates<'q> {
    pub(in crate::tools::local_search) fn new(
        written: &'q LocalSearchQuery,
        scan: &Scanned,
    ) -> Self {
        let listed = scan
            .reusable
            .as_ref()
            .filter(|_| scan.parsed.files.is_empty())
            .and_then(|walked| walked.searched_files())
            .map(<[PathBuf]>::to_vec);
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
        let scope_counted = scan.stream.is_none()
            && scan.seek.is_none()
            && scan.paused.is_none()
            && stats.cap_reasons.is_empty()
            && stats.capped != Some(true)
            && stats.error_count.unwrap_or(0) == 0;
        Self {
            written,
            listed,
            term_hits,
            scope_counted,
            reading: OnceCell::new(),
            read: Cell::new(Pass::Unrun),
        }
    }

    /// No list: every reading walks.
    #[cfg(test)]
    pub(super) fn unlisted(written: &'q LocalSearchQuery) -> Self {
        Self {
            written,
            listed: None,
            term_hits: None,
            scope_counted: false,
            reading: OnceCell::new(),
            read: Cell::new(Pass::Unrun),
        }
    }

    /// The hint of an empty search whose case rule decided it: a case-sensitive
    /// search (`caseMode:"sensitive"`, or `smart` with an uppercase letter)
    /// that the same files match with case ignored. `None` otherwise, without
    /// a probe when the search already ignored case or lists files without the
    /// term, whose page is empty because the term is there. The regex reading
    /// shares the pass when the row can still offer it (`offers`).
    pub(in crate::tools::local_search) fn case_hint(
        &self,
        query: &LocalSearchQuery,
        paths: &PathPolicy,
        search_options: &TextSearchOptions,
        offers: bool,
        cancel: &impl CancellationCheck,
    ) -> Option<String> {
        if local_search_query::view(query) == View::FilesWithout {
            return None;
        }
        let sensitive = match local_search_query::case_mode(query) {
            octocode_contract_types::CaseMode::Sensitive => true,
            octocode_contract_types::CaseMode::Smart => local_search_query::patterns(query)
                .0
                .iter()
                .any(|term| term.chars().any(char::is_uppercase)),
            octocode_contract_types::CaseMode::Insensitive => false,
        };
        if !sensitive {
            return None;
        }
        let mut probe = search_options.probe(SearchOutput::FilesWithMatch);
        probe.case = CaseRule::Insensitive;
        let reading = offers.then(|| self.reading(paths)).flatten();
        let mut asked = vec![&probe];
        asked.extend(reading.map(|reading| &reading.probe));
        let found = self.search(&asked, paths, cancel);
        if reading.is_some() {
            self.read.set(found[1].map_or(Pass::Empty, Pass::Found));
        }
        found[0].filter(|found| found.count > 0).map(|found| {
            format!(
                "No case-sensitive match, but {found} file(s) match with case ignored; caseMode:\"insensitive\" finds them."
            )
        })
    }

    /// Offer the regex reading of an empty literal search when the same files
    /// match it: an agent writing `a|b` or `x[0-9]` usually means a regex, but
    /// `arr[i]` and `char*` are code, so the row stays the literal answer and
    /// the caller chooses.
    pub(in crate::tools::local_search) fn offer_regex_reading(
        &self,
        paths: &PathPolicy,
        row: &mut LocalSearchResult,
        cancel: &impl CancellationCheck,
    ) {
        if row.status != SearchStatus::Empty || row.stats.files_searched == 0 {
            return;
        }
        let Some(reading) = self.reading(paths) else {
            return;
        };
        let found = match self.read.get() {
            Pass::Unrun => self.search(&[&reading.probe], paths, cancel)[0],
            Pass::Empty => None,
            Pass::Found(found) => Some(found),
        };
        let Some(found) = found.filter(|found| found.count > 0) else {
            return;
        };
        let lead =
            crate::tools::leads::skipped::lead(crate::id::ToolId::LocalSearch, &reading.query, &[]);
        if let Some(map) = row.next.get_or_insert_with(|| json!({})).as_object_mut() {
            map.insert("asRegex".into(), lead);
        }
        row.hints.push(format!(
            "No literal match, but {found} file(s) match this read as a regex; the related[] call sets regexEngine:\"rust\"."
        ));
    }

    /// Name each term of a several-term literal request that no file holds
    /// as written and that reads as a regex matching files, beside the
    /// terms that hit: the row is a success, and a dead term that was meant
    /// as a regex would otherwise stay silent. A scan that counted its whole
    /// scope settles every term at zero; any other scan's zero is confirmed
    /// by a literal probe, and a page that scan only partly read (a later
    /// page cannot name what an earlier one hid) offers nothing.
    pub(in crate::tools::local_search) fn offer_dead_terms(
        &self,
        paths: &PathPolicy,
        row: &mut LocalSearchResult,
        cancel: &impl CancellationCheck,
    ) {
        use octocode_engine::search::text_pattern::regex_spells_itself;
        let written = self.written;
        let terms = &local_search_query::patterns(written).0;
        let view = local_search_query::view(written);
        // A files view stops each file at its first match, so its counts of
        // the other terms are floors.
        let settled = self.scope_counted && view != View::Files;
        let paged = row
            .next
            .as_ref()
            .is_some_and(|next| next.get("nextPage").is_some());
        if row.status == SearchStatus::Empty
            || (paged && !settled)
            || terms.len() < 2
            || local_search_query::regex_mode(written) != RegexEngine::Literal
            || view == View::FilesWithout
            || local_search_query::invert_match(written) == Some(true)
            || local_search_query::snapshot(written).is_some()
            || local_search_query::page(written) > 1
        {
            return;
        }
        // A term the main scan found in a file is no dead term: only the
        // terms it never saw are candidates.
        let candidates = terms
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.term_hits
                    .as_ref()
                    .and_then(|hits| hits.get(*index))
                    .is_none_or(|hits| *hits == 0)
            })
            .map(|(_, term)| term)
            .filter(|term| !regex_spells_itself(term) && regex::Regex::new(term).is_ok())
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return;
        }
        let Ok(root) = paths.validate(local_search_query::path(written)) else {
            return;
        };
        let probe = |terms: &[&str], engine: RegexEngine, output: SearchOutput| {
            let mut fields = serde_json::to_value(written).ok()?;
            fields["matchString"] = json!(terms);
            fields["regexEngine"] = json!(engine);
            let query = serde_json::from_value::<LocalSearchQuery>(fields).ok()?;
            let joined = local_search_query::normalized(&query).ok()?;
            let mut options =
                crate::tools::local_search::executor::search_options(&joined, &root.canonical)
                    .probe(output);
            // The terms a probe of several counts one by one.
            options.terms = local_search_query::term_patterns(&query);
            Some(options)
        };
        let dead: Vec<&str> = if settled && self.term_hits.is_some() {
            candidates.iter().map(|term| term.as_str()).collect()
        } else {
            let literal = candidates
                .iter()
                .filter_map(|term| {
                    probe(
                        &[term.as_str()],
                        RegexEngine::Literal,
                        SearchOutput::FilesWithMatch,
                    )
                    .map(|probe| (term.as_str(), probe))
                })
                .collect::<Vec<_>>();
            let asked = literal.iter().map(|(_, probe)| probe).collect::<Vec<_>>();
            let found = self.search(&asked, paths, cancel);
            literal
                .iter()
                .zip(&found)
                .filter(|(_, found)| found.is_some_and(|found| found.count == 0 && !found.cut))
                .map(|((term, _), _)| *term)
                .collect()
        };
        for (term, found) in dead
            .iter()
            .zip(self.as_regexes(&dead, &probe, paths, cancel))
        {
            if let Some(found) = found.filter(|found| found.count > 0) {
                row.warnings.push(format!(
                    "No literal match for `{term}`, but {found} file(s) match it read as a regex; regexEngine:\"rust\" reads every term that way."
                ));
            }
        }
    }

    /// How many files hold each of `dead` read as a regex: one pass over the
    /// listed files, or one walk that counts every term, whatever their number.
    fn as_regexes(
        &self,
        dead: &[&str],
        probe: &impl Fn(&[&str], RegexEngine, SearchOutput) -> Option<TextSearchOptions>,
        paths: &PathPolicy,
        cancel: &impl CancellationCheck,
    ) -> Vec<Option<Files>> {
        let singles = dead
            .iter()
            .map(|term| probe(&[*term], RegexEngine::Rust, SearchOutput::FilesWithMatch))
            .collect::<Vec<_>>();
        if singles.iter().any(Option::is_none) {
            return vec![None; dead.len()];
        }
        let singles = singles.into_iter().flatten().collect::<Vec<_>>();
        let asked = singles.iter().collect::<Vec<_>>();
        let listable_pass = self
            .listed
            .as_deref()
            .is_some_and(|_| asked.iter().all(|options| listable(options)));
        if listable_pass || dead.len() < 2 {
            return self.search(&asked, paths, cancel);
        }
        // Every term counts in one walk of the joined terms.
        let names = dead.to_vec();
        let Some(joined) = probe(&names, RegexEngine::Rust, SearchOutput::CountLines) else {
            return self.search(&asked, paths, cancel);
        };
        let defaults = query_walk(self.written, Default::default()).default_excludes;
        let Some((found, cut, _)) = probe_walked(joined, paths, defaults, None, cancel) else {
            return vec![None; dead.len()];
        };
        (0..dead.len())
            .map(|index| {
                let count = found
                    .files
                    .iter()
                    .filter(|file| file.term_counts.get(index).is_some_and(|hits| *hits > 0))
                    .count();
                Some(Files { count, cut })
            })
            .collect()
    }

    /// The written request read as a regex. `None` when no term holds regex
    /// syntax or a term fails to compile, on a shape a regex would not
    /// answer, or when the request cannot be restated.
    fn reading(&self, paths: &PathPolicy) -> Option<&Reading> {
        self.reading
            .get_or_init(|| {
                let written = self.written;
                let terms = &local_search_query::patterns(written).0;
                if local_search_query::regex_mode(written) != RegexEngine::Literal
                    || local_search_query::view(written) == View::FilesWithout
                    || local_search_query::invert_match(written) == Some(true)
                    || local_search_query::snapshot(written).is_some()
                    || terms
                        .iter()
                        .all(|term| regex::escape(term) == term.as_str())
                    || terms.iter().any(|term| regex::Regex::new(term).is_err())
                {
                    return None;
                }
                let mut fields = serde_json::to_value(written).ok()?;
                fields["regexEngine"] = json!(RegexEngine::Rust);
                let query = serde_json::from_value::<LocalSearchQuery>(fields).ok()?;
                let joined = local_search_query::normalized(&query).ok()?;
                let root = paths.validate(local_search_query::path(written)).ok()?;
                let probe =
                    crate::tools::local_search::executor::search_options(&joined, &root.canonical)
                        .probe(SearchOutput::FilesWithMatch);
                Some(Reading { query, probe })
            })
            .as_ref()
    }

    /// What each of `asked` finds, `None` for a search that cannot run: one
    /// pass over the listed files, or else a probe walk each.
    fn search(
        &self,
        asked: &[&TextSearchOptions],
        paths: &PathPolicy,
        cancel: &impl CancellationCheck,
    ) -> Vec<Option<Files>> {
        if let Some(files) = self
            .listed
            .as_deref()
            .filter(|_| asked.iter().all(|options| listable(options)))
        {
            #[cfg(test)]
            LISTED_PASSES.with(|passes| passes.set(passes.get() + 1));
            let (files, cut) = within_work(files);
            let counts = count_listed(asked, files, &|| cancel.check().is_err());
            if let Ok(counts) = counts {
                return counts
                    .into_iter()
                    .map(|count| Some(Files { count, cut }))
                    .collect();
            }
        }
        let defaults = query_walk(self.written, Default::default()).default_excludes;
        asked
            .iter()
            .map(|options| {
                let (found, cut, _) =
                    probe_walked((*options).clone(), paths, defaults, None, cancel)?;
                Some(Files {
                    count: found.files.len(),
                    cut,
                })
            })
            .collect()
    }
}

#[cfg(test)]
thread_local! {
    static LISTED_PASSES: Cell<usize> = const { Cell::new(0) };
}

/// Passes over a listed file set this thread ran.
#[cfg(test)]
pub(super) fn listed_passes() -> usize {
    LISTED_PASSES.with(Cell::get)
}
