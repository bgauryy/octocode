//! The per-batch finalize seam (GH-G1).
use super::DomainDispatcher;
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::{ToolFamily, ToolId};
use crate::response::outcome::DomainResult;

/// Fills the fields a tool family deferred across a batch's rows.
type FillDeferred =
    fn(&DomainDispatcher, &mut [DomainResult], &ExecutionContext) -> Result<(), ExecutionError>;

/// The family's [`FillDeferred`], if its rows defer any field to the batch.
/// None does yet; lane C registers GitHub's batched commit timestamps.
fn deferred_fields(family: ToolFamily) -> Option<FillDeferred> {
    match family {
        ToolFamily::GitHub | ToolFamily::Local | ToolFamily::Remote => None,
    }
}

impl DomainDispatcher {
    /// Runs once per batch, after every row's result exists and before the
    /// response stage shapes them, so a tool family can fill fields it
    /// deferred while its rows ran (one provider request for the whole
    /// batch instead of one per row).
    pub(crate) fn finalize_batch(
        &self,
        tool: ToolId,
        rows: &mut [DomainResult],
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        context.check()?;
        match deferred_fields(tool.family()) {
            Some(fill) => fill(self, rows, context),
            None => Ok(()),
        }
    }
}
