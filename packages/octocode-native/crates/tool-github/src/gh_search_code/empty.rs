//! Empty-search diagnosis and executable recovery routes.
use super::{code_output, leads};
use octocode_contract_types::GhSearchCodeQuery;
use octocode_github::{GitHubProvider, ProviderError, RepositoryMetadata, RequestContext};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, ToolData, fresh_query_row, query_row};
use serde_json::json;

/// Why a search is empty, with the route that verifies it.
pub(super) async fn page(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    checked: Option<RepositoryMetadata>,
    output: &mut ToolData,
) -> Result<(), ProviderError> {
    let broader = leads::broaden_search(query, provider.transport.code_search_room(context));
    let value = &mut output.data;
    let default_branch = code_output::empty_scope(
        value,
        &mut output.diagnostics,
        query,
        broader.is_some(),
        &provider.transport,
        checked,
        context,
    )
    .await?;
    let default_branch = default_branch.as_deref();
    if value.get("hints").is_none() && code_output::path_mode_given_code(query) {
        // Path matching never sees file contents: search them instead.
        let mut content = fresh_query_row(ToolId::GhSearchCode, &query_row(query, &[]));
        content["matchIn"] = json!(["content"]);
        value["calls"]["findCode"] = Continuation::related(ToolId::GhSearchCode, content).build();
    }
    let hints = code_output::empty_hints(query, default_branch);
    if value.get("hints").is_none() && !hints.is_empty() {
        octocode_runtime_base::response::result::add_hints(value, hints);
        if value["calls"].get("read").is_none()
            && code_output::unsearched_ref(query, default_branch).is_some()
            && let Some(read) = leads::unsearched_ref_read(query)
        {
            value["calls"]["read"] = read;
        }
    }
    if let Some(broader) = broader {
        octocode_tools_shared::leads::broaden::set_broadened(
            value,
            &query_row(query, &[]),
            broader,
        );
    }
    if let Some(split) = leads::or_split(query) {
        octocode_runtime_base::response::result::add_hints(
            value,
            [
                "keywords hold OR, which code search does not support: run the related[] ghSearchCode request, one row per alternative.",
            ],
        );
        value["calls"]["repair"] = split;
    }
    Ok(())
}
