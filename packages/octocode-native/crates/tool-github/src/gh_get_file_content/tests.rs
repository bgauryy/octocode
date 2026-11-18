use super::shape::row_envelope_chars;
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_contract_types::ErrorCode;
use octocode_github::RetryPolicy;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_test_support::mock_http::mount_json;
use octocode_tools_shared::gh_shared::test_support::{
    default_context, fixture_context, mock_provider,
};
use octocode_tools_shared::source_view::local_query::local_query;
use octocode_tools_shared::source_view::{ReadStatus, process_fetched_content};
use serde_json::json;
use std::{path::Path, time::Duration};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

mod continuations;
mod lfs;
mod long_reads;
mod misses;
mod missing_repository;
pub mod paths;
mod redaction;
mod renamed;
mod rows;
mod selection;
mod windows;
mod worker;

/// Shaping over the test scanner [`Safe`]; a ranked read judges with
/// `judge` (a plain read never calls it).
fn shaping(
    judge: impl octocode_tools_shared::selection::SelectionJudge + 'static,
) -> Shaping<Safe> {
    let selection = octocode_tools_shared::test_support::selection_fake::runtime(
        judge,
        tokio::runtime::Handle::current(),
        Duration::from_secs(10),
    );
    Shaping {
        security: Safe,
        context: selection.context.clone(),
        selection,
    }
}

async fn execute_default_regex(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    request_context: &RequestContext,
    window: Option<usize>,
) -> Result<GhGetFileContentResult, ProviderError> {
    let select_all = octocode_tools_shared::test_support::selection_fake::SelectAll;
    execute(
        provider,
        query,
        request_context,
        window,
        shaping(select_all),
        &|_| false,
        &mut None,
    )
    .await
    .map(|read| read.expect("no target is withheld"))
}

#[derive(Clone)]
struct Safe;
impl ContentScan for Safe {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((text.replace("TOKEN", "[REDACTED]"), vec![]))
    }
}

async fn read(provider: &GitHubProvider, query: Value) -> GhGetFileContentResult {
    let query = query;
    let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
    execute_default_regex(provider, &query, &default_context(), Some(50_000))
        .await
        .expect("result")
}

async fn run_default(
    provider: &GitHubProvider,
    query: Value,
) -> Result<ToolData, octocode_tools_shared::gh_shared::GhFailure> {
    let query = query;
    let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
    run(
        provider,
        &query,
        Ok(&default_context()),
        None,
        shaping(octocode_tools_shared::test_support::selection_fake::SelectAll),
        &|_| false,
    )
    .await
    .map(|read| read.expect("no target is withheld"))
}
