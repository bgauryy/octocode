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
mod windows;
mod worker;

/// A call context that ends in ten seconds.
fn test_context() -> octocode_runtime_base::execution::ExecutionContext {
    octocode_runtime_base::execution::ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: Some(std::time::Instant::now() + Duration::from_secs(10)),
        response_window: None,
        github_credential: None,
    }
}

/// Shaping over the test scanner [`Safe`].
fn shaping() -> Shaping<Safe> {
    Shaping {
        security: Safe,
        context: test_context(),
    }
}

async fn execute_default_regex(
    provider: &GitHubProvider,
    query: &GhGetFileContentQuery,
    request_context: &RequestContext,
    window: Option<usize>,
) -> Result<GhGetFileContentResult, ProviderError> {
    execute(
        provider,
        query,
        request_context,
        window,
        shaping(),
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
        shaping(),
        &|_| false,
    )
    .await
    .map(|read| read.expect("no target is withheld"))
}
