use super::rows::{history_error, tool_row};
use super::*;
use crate::tools::gh_get_file_content;
use octocode_contract_types::GhGetFileContentQuery;
mod access;
mod errors;
mod rate_limits;

fn search_row(error: ProviderError) -> DomainResult {
    tool_row(Err(crate::tools::gh_shared::search_failure(error)))
}

fn file_row(error: ProviderError, query: Value) -> DomainResult {
    let query: GhGetFileContentQuery = serde_json::from_value(query).expect("file query");
    tool_row(Err(gh_get_file_content::errors::failure(
        error, &query, None,
    )))
}
