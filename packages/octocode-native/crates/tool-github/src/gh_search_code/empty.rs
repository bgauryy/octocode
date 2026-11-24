//! Empty-search diagnosis and executable recovery routes.
use super::search_page::FileRead;
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
    let scope = code_output::empty_scope(
        value,
        &mut output.diagnostics,
        query,
        &provider.transport,
        checked,
        context,
    )
    .await?;
    // An archived repository's empty answer is its coverage, with the
    // listing as recovery: no search-side lead or hint applies to it.
    let unindexed = scope.archived;
    let default_branch = scope.default_branch.as_deref();
    if !unindexed && code_output::path_mode_given_code(query) {
        // Path matching never sees file contents: search them instead.
        let mut content = fresh_query_row(ToolId::GhSearchCode, &query_row(query, &[]));
        content["matchIn"] = json!(["content"]);
        value["calls"]["findCode"] = Continuation::related(ToolId::GhSearchCode, content).build();
    }
    let hints = code_output::empty_hints(query, default_branch);
    if !unindexed && !hints.is_empty() {
        octocode_runtime_base::response::result::add_hints(value, hints);
        if value["calls"].get("read").is_none()
            && code_output::unsearched_ref(query, default_branch).is_some()
            && let Some(read) = leads::unsearched_ref_read(query)
        {
            value["calls"]["read"] = read;
        }
    }
    let keywords: Vec<&str> = query
        .keywords
        .iter()
        .map(|keyword| keyword.as_str())
        .collect();
    octocode_runtime_base::response::result::add_hints(
        value,
        code_output::qualifier_keyword_hints(&keywords),
    );
    if let Some(broader) = broader {
        octocode_tools_shared::leads::broaden::set_broadened(
            value,
            &query_row(query, &[]),
            broader,
        );
    }
    or_repair(query, value);
    Ok(())
}

/// The empty page of a row answered from the file its `path` names: the
/// file was read whole at its commit, so the answer is complete over it, not
/// over an index; keywords spelled as OR keep their repair. A path the
/// default branch's head does not hold in any case of its name is complete
/// over that commit too: its folder's listing is the one route on, and no
/// lead reads or searches the missing path again.
pub(super) fn read_file(query: &GhSearchCodeQuery, read: &FileRead, output: &mut ToolData) {
    let (path, commit) = (read.path.as_str(), read.commit.as_str());
    if !read.found {
        output.data["coverage"] = json!({
            "state": "complete",
            "reason": format!("{path} is not at {commit}"),
        });
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        let tree = json!({
            "operation": "tree",
            "owner": query.owner,
            "repo": query.repo,
            "path": parent,
        });
        output.data["calls"]["viewTree"] = Continuation::related(ToolId::GhStructure, tree).build();
        return;
    }
    output.data["coverage"] = json!({
        "state": "complete",
        "reason": format!("{path} was read whole at {commit}: it does not hold every keyword"),
    });
    let keywords: Vec<&str> = query
        .keywords
        .iter()
        .map(|keyword| keyword.as_str())
        .collect();
    octocode_runtime_base::response::result::add_hints(
        &mut output.data,
        code_output::qualifier_keyword_hints(&keywords),
    );
    or_repair(query, &mut output.data);
}

/// Keywords spelled as OR, which code search does not support: the split
/// into one row per alternative.
fn or_repair(query: &GhSearchCodeQuery, value: &mut serde_json::Value) {
    if let Some(split) = leads::or_split(query) {
        octocode_runtime_base::response::result::add_hints(
            value,
            [
                "keywords hold OR, which code search does not support: run the related[] ghSearchCode request, one row per alternative.",
            ],
        );
        value["calls"]["repair"] = split;
    }
}
