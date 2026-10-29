//! What evaluating one walked file yields: an entry, or a named skip.
use super::super::GraphScanError;
use crate::types::GraphFactsScanDiagnostic;

pub(super) enum GraphFactsScanOutcome<T> {
    /// A parsed file and what the caller's map kept of it (`None`: nothing).
    Entry(String, Option<T>),
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
/// outgrew what one file may hold.
pub(super) enum Mapped<T> {
    Kept(Option<T>),
    OverBudget,
}

/// A mapped file's outcome: an over-budget file is a named gap, never a
/// silent drop.
pub(super) fn mapped_outcome<T>(
    relative_path: String,
    mapped: Mapped<T>,
) -> Result<Option<GraphFactsScanOutcome<T>>, GraphScanError> {
    Ok(Some(match mapped {
        Mapped::Kept(kept) => GraphFactsScanOutcome::Entry(relative_path, kept),
        Mapped::OverBudget => skipped(
            relative_path,
            SOURCE_BUDGET_EXCEEDED,
            "file's source correspondence exceeds the per-file budget",
        ),
    }))
}

/// The skip code of a file whose source correspondence outgrows the
/// per-file budget, whichever stage measured it.
pub const SOURCE_BUDGET_EXCEEDED: &str = "graph.scan.sourceBudgetExceeded";
