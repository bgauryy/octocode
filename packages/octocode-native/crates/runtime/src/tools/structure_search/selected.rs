//! What the shared response stages ask structureSearch about its output.
use crate::id::ToolId;
use crate::response::output::{ToolOutput, retain_lead_reads};
use serde_json::Value;
use std::collections::HashSet;

/// This tool's selection finish for the shared response stages.
pub(crate) struct Output;

impl ToolOutput for Output {
    /// A listing's read lead names one listed file: a read of an entry
    /// classification cut goes with it.
    fn finish_selection(
        &self,
        data: &mut Value,
        _query: &Value,
        _hidden: &[Value],
        removed: &[Value],
    ) {
        let cut = removed
            .iter()
            .filter_map(|unit| unit["path"].as_str())
            .collect::<HashSet<_>>();
        retain_lead_reads(data, ToolId::LocalFetch, |read| {
            read["path"].as_str().is_none_or(|path| !cut.contains(path))
        });
    }
}
