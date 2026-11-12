//! Rebuilding one scanned file's matches for the page that shows them.

use std::path::Path;

use super::file_scan::read_source;
use crate::error::{Error, Result};
use crate::structural::types::{StructuralDetailedMatch, StructuralDiagnostic};
use crate::text::analysis::{AdmittedSource, MapBudget, MapLimits};

/// One file's matches rebuilt for the page that shows them ([`file_matches`]).
pub enum Rebuilt {
    /// The scan's own matches, in its order.
    Matches(Vec<StructuralDetailedMatch>),
    /// The file no longer hashes to the scan's digest or no longer reads as
    /// the scan read it.
    Changed,
    /// The scan's bytes did not finish matching this time (an execution
    /// limit, such as the match deadline): its diagnostics say why.
    Incomplete(Vec<StructuralDiagnostic>),
}

impl Rebuilt {
    /// What matching the scan's own bytes gave: its matches, or the limit
    /// that cut it (the same bytes and query only differ in time spent).
    pub(in crate::structural) fn of(
        result: crate::structural::StructuralSearchDetailedResult,
    ) -> Self {
        if result.status == "ok" {
            Self::Matches(result.matches)
        } else {
            Self::Incomplete(result.diagnostics)
        }
    }
}

/// The matches of one file a [`search_files`](super::search_files) scan evaluated, rebuilt for
/// the page that shows them (a [`StructuralDemand::Priced`] scan kept only
/// their prices): the file is read through the scan's bounded reader and
/// matched with the same query under `extension`'s grammar, so the matches
/// are the scan's own, in its order.
pub fn file_matches(
    path: &Path,
    extension: &str,
    pattern: Option<&str>,
    rule: Option<&serde_json::Value>,
    source_sha256: &str,
    (max_file_bytes, judge): (u64, Option<&crate::structural::TextJudge>),
    stop: &crate::search::walk::Stop<'_>,
) -> Result<Rebuilt> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Ok(bytes) = read_source(path, max_file_bytes) else {
            return Ok(Rebuilt::Changed);
        };
        if crate::digest::sha256(&bytes) != source_sha256 {
            return Ok(Rebuilt::Changed);
        }
        let path_string = path.to_string_lossy();
        // Admission is bounded by the map limits and the caller's
        // cancellation (its request deadline), not a clock of its own.
        let mut budget = MapBudget::new(
            MapLimits::default(),
            crate::signatures::extractor::whole_file_deadline(),
            stop,
        );
        let content = match AdmittedSource::from_bytes(&bytes, &path_string, &mut budget) {
            Ok(content) => content,
            Err(crate::text::analysis::SourceError::Refused(_)) => return Ok(Rebuilt::Changed),
            Err(error) => return Err(Error::from(error)),
        };
        // Matched as the scan matched it.
        let (content, shown) = super::file_scan::judged(content, judge, path);
        let mut result = crate::structural::evaluate_source(
            content.text(),
            &path.to_string_lossy(),
            extension,
            pattern,
            rule,
        );
        let mut binder = crate::structural::SourceBinder::with_budget(&content, budget);
        for matched in &mut result.matches {
            if let Err(error) = binder.bind_match(matched) {
                return Err(Error::from(error));
            }
        }
        if let Some(shown) = &shown {
            crate::structural::restore_texts(&mut result.matches, content.text(), shown);
        }
        Ok(Rebuilt::of(result))
    }))
    .map_err(|_| Error::new("structural file match failed on pathological input"))?
}
