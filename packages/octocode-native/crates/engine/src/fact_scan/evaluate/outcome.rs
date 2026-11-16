//! What evaluating one walked file yields: an entry, or a named skip.
use super::super::GraphScanError;
use crate::types::GraphFactsScanDiagnostic;

pub(super) enum GraphFactsScanOutcome<T> {
    /// A parsed file, what the caller's map kept of it (`None`: nothing), and
    /// why its text is not the file's UTF-8 when it was decoded
    /// ([`crate::text::analysis::AdmittedSource::warning`]).
    Entry(String, Option<T>, Option<&'static str>),
    Skipped(GraphFactsScanDiagnostic),
}

pub(super) fn skipped<T>(
    relative_path: String,
    code: &str,
    message: &str,
) -> GraphFactsScanOutcome<T> {
    GraphFactsScanOutcome::Skipped(GraphFactsScanDiagnostic {
        relative_path,
        code: code.to_owned(),
        message: message.to_owned(),
    })
}

/// What mapping one file kept: the caller's map result (`None`: the map
/// filtered it out), or nothing because the file's source correspondence
/// outgrew what one file may hold ([`crate::text::analysis::MapBudget::exceeded`]).
pub(super) enum Mapped<T> {
    Kept(Option<T>),
    /// The limit passed (a [`crate::text::analysis::SourceError::LimitExceeded`]
    /// name) and its wording.
    OverBudget(&'static str, String),
    /// The scan cannot keep this file beside what it kept: the scan's
    /// refusal ([`crate::fact_scan::ScanMemory::refused`]), not the file's
    /// skip.
    Refused,
}

/// A mapped file's outcome: an over-budget file is a named gap, never a
/// silent drop.
pub(super) fn mapped_outcome<T>(
    relative_path: String,
    mapped: Mapped<T>,
    decoded: Option<&'static str>,
) -> Result<Option<GraphFactsScanOutcome<T>>, GraphScanError> {
    Ok(match mapped {
        Mapped::Kept(kept) => Some(GraphFactsScanOutcome::Entry(relative_path, kept, decoded)),
        Mapped::OverBudget(_, exceeded) => Some(skipped(
            relative_path,
            SOURCE_BUDGET_EXCEEDED,
            &over_budget_message(&exceeded),
        )),
        // The scan stops with the caller's refusal; the file is no outcome.
        Mapped::Refused => None,
    })
}

/// The skip message of a file whose source correspondence passed a limit
/// ([`crate::text::analysis::MapBudget::exceeded`]), whichever stage
/// measured it.
#[must_use]
pub fn over_budget_message(exceeded: &str) -> String {
    format!("file's source correspondence {exceeded}; none of its facts are in the graph")
}

/// The skip code of a file whose source correspondence outgrows the
/// per-file budget, whichever stage measured it.
pub const SOURCE_BUDGET_EXCEEDED: &str = "graph.scan.sourceBudgetExceeded";

/// The code of a file indexed from decoded text (Latin-1 or lossy): it is in
/// the graph; its message is the shared decoding sentence.
pub const DECODED: &str = "graph.scan.decoded";

/// The skip code of a file whose work panicked, whichever phase did.
pub const PANICKED: &str = "graph.scan.panicked";

/// The skip of a file whose work panicked: named with its phase and the
/// panic's location, never its message, and unassessed.
#[must_use]
pub fn panicked(
    relative_path: String,
    contained: &crate::contain::Contained,
) -> GraphFactsScanDiagnostic {
    GraphFactsScanDiagnostic {
        relative_path,
        code: PANICKED.to_owned(),
        message: format!("{contained}; the file is unassessed: none of its facts are in the graph"),
    }
}

/// A contained panic of one file's work as its named skip.
pub(super) fn contained<T>(
    relative_path: String,
    contained: &crate::contain::Contained,
) -> Result<Option<GraphFactsScanOutcome<T>>, GraphScanError> {
    Ok(Some(GraphFactsScanOutcome::Skipped(panicked(
        relative_path,
        contained,
    ))))
}

/// An error of `phase` that belongs to the file (its source or the map its
/// facts carry is invalid) is that file's named skip; any other error
/// (cancellation, a deadline, a budget) ends the scan.
pub(super) fn failed<T>(
    relative_path: String,
    phase: crate::contain::Phase,
    error: GraphScanError,
) -> Result<Option<GraphFactsScanOutcome<T>>, GraphScanError> {
    use crate::text::analysis::SourceError;
    match error {
        GraphScanError::Source(SourceError::InvalidMap(_) | SourceError::Refused(_)) => {
            Ok(Some(skipped(
                relative_path,
                "graph.scan.extractFailed",
                &format!(
                    "{} phase failed: {error}; the file is unassessed: none of its facts are in the graph",
                    phase.label()
                ),
            )))
        }
        error => Err(error),
    }
}
