//! Contract and transport tests for response rows.

use super::minimize::{contract_data_variants, data_variants_of, reports_scan_totals};
use super::*;
use octocode_contract_types::ErrorCode;

mod compaction;
mod continuation;
mod error_hints;
mod hints;
mod minimal_rows;
mod page_keeping;
mod row_state;
mod workspace_paths;

fn minimized(tool: ToolId, query: Value, data: Value) -> Value {
    let mut row = json!({"index":0,"data":data,"cache":1});
    // The engine order: the extras stage, then the minimizer.
    super::super::extras::prune_row(&mut row, tool, &query);
    minimize_row(&mut row, tool, &query);
    row
}
