use super::rows::{history_error, tool_row};
use super::*;
use crate::tools::gh_get_file_content;
mod access;
mod errors;
mod rate_limits;

fn search_row(error: ProviderError) -> DomainResult {
    tool_row(Err(crate::tools::gh_shared::search_failure(error)))
}

fn file_row(error: ProviderError, query: Value) -> DomainResult {
    let query: gh_get_file_content::GhGetFileContentQuery =
        serde_json::from_value(query).expect("file query");
    tool_row(Err(gh_get_file_content::errors::failure(
        error, &query, None,
    )))
}

/// The row as the response stage publishes it: the shared recovery of
/// its `errorCode` when the arm stated no hint and no lead.
fn staged(tool: ToolId, result: &DomainResult) -> Value {
    let query = json!({"owner":"a","repo":"b"});
    let mut row =
        crate::response::rows::result_row(tool, 0, &query, result.data.clone(), result.status);
    crate::response::rows::apply_hint_policy(&mut row, tool, &query);
    row["data"].take()
}
