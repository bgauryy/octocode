use crate::runtime::tool_output::ToolOutputOf;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::security::CheckedQuery;
use octocode_tools_shared::selection::Runtime;
use octocode_tools_shared::selection::apply_sanitized_rows;
use octocode_tools_shared::selection::provider_page::strip_plan_from_tool_continuations;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Selects the one `ghSearchCode` row of `output`, judged for `query`.
fn apply_code_rows(
    output: &mut Value,
    query: Value,
    response_query: &mut Value,
    runtime: &Runtime,
) -> Result<(), octocode_runtime_base::execution::ExecutionError> {
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        ToolId::GhSearchCode.output(),
        output,
        &[CheckedQuery::checked(query)],
        &[0],
        response_query,
        runtime,
    )
}
mod candidate_pages;
mod code_page;
mod code_page_failures;
mod row_pages;
