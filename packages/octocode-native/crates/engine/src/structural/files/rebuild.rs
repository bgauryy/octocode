//! Rebuilding one scanned file's matches for the page that shows them.

use std::path::Path;

use super::file_scan::read_source;
use crate::error::{Error, Result};
use crate::structural::types::{StructuralDetailedMatch, StructuralDiagnostic};
use crate::text::{Decoding, admit};

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
    max_file_bytes: u64,
) -> Result<Rebuilt> {
    std::panic::catch_unwind(|| {
        let Ok(bytes) = read_source(path, max_file_bytes) else {
            return Rebuilt::Changed;
        };
        if crate::digest::sha256(&bytes) != source_sha256 {
            return Rebuilt::Changed;
        }
        let Ok(content) = admit(&bytes, Decoding::Unicode) else {
            return Rebuilt::Changed;
        };
        Rebuilt::of(crate::structural::evaluate_source(
            &content.text,
            &path.to_string_lossy(),
            extension,
            pattern,
            rule,
        ))
    })
    .map_err(|_| Error::new("structural file match failed on pathological input"))
}
