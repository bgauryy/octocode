use super::super::filter::FileFilter;
use super::super::window::{
    COMMIT_FILE_BATCH, MAX_COMMIT_FILE_BATCHES, WindowSpec, commit_file_items, load_window_with,
};
use super::super::{HistoryItemRequest, gh_get_history_item_query};
use crate::id::ToolId;
use crate::response::result::Continuation;
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Value, json};

pub(super) async fn first_page(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    sha: &str,
    filter: FileFilter<'_>,
    page_size: usize,
    context: &RequestContext,
) -> Result<(Vec<Value>, Value), ProviderError> {
    let mut loaded = load_window_with(
        transport,
        &[
            "repos",
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            "commits",
            sha,
        ],
        WindowSpec {
            batch_size: COMMIT_FILE_BATCH,
            max_batches: MAX_COMMIT_FILE_BATCHES,
            page: 1,
            page_size,
            filtered: !filter.is_trivial(),
            provider_total: None,
            prefetched: None,
        },
        |file| filter.matches(file),
        commit_file_items,
        context,
    )
    .await?;
    loaded.mark_commit_file_limit();
    Ok(loaded.state.paginate(
        filter.apply(loaded.items, false).kept,
        Some(1),
        Some(page_size),
    ))
}

pub(super) fn next_page(
    query: &HistoryItemRequest,
    sha: &str,
    page: u64,
    page_size: usize,
) -> Value {
    let mut row = json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),
        "repo":gh_get_history_item_query::repo(query),"ref":sha,
        "sections":["files"],"page":page,"pageSize":page_size});
    for key in ["include", "fileStatus", "minChanges"] {
        if let Some(value) = query.public.get(key) {
            row[key] = value.clone();
        }
    }
    Continuation::continued(ToolId::GhGetHistoryItem, row).build()
}
