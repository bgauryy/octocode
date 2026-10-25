use super::local_query::local_query;
use super::shape::row_envelope_chars;
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::scan::{MemoizedScan, SanitizedViewMemo};
use crate::tools::gh_shared::test_support::{default_context, fixture_context, mock_provider};
use crate::tools::source_view::{ReadStatus, process_fetched_content};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_contract_types::ErrorCode;
use octocode_github::RetryPolicy;
use octocode_github::mock_http::mount_json;
use serde_json::json;
use std::{path::Path, time::Duration};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

mod continuations;
mod misses;
mod paths;
mod redaction;
mod rows;
mod selection;
mod windows;

/// Shaping over the test scanner [`Safe`]; a ranked read judges with
/// `judge` (a plain read never calls it).
fn shaping(judge: impl crate::tools::selection::SelectionJudge + 'static) -> Shaping<Safe> {
    let selection = crate::tools::test_support::selection_fake::runtime(
        judge,
        tokio::runtime::Handle::current(),
        Duration::from_secs(10),
    );
    Shaping {
        security: Safe,
        memo: Arc::new(SanitizedViewMemo::new()),
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
    let select_all = crate::tools::test_support::selection_fake::SelectAll;
    execute(
        provider,
        query,
        request_context,
        window,
        shaping(select_all),
    )
    .await
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
) -> Result<ToolData, crate::tools::gh_shared::GhFailure> {
    let query = query;
    let query: GhGetFileContentQuery = serde_json::from_value(query).expect("query");
    run(
        provider,
        &query,
        Ok(&default_context()),
        None,
        shaping(crate::tools::test_support::selection_fake::SelectAll),
    )
    .await
}
