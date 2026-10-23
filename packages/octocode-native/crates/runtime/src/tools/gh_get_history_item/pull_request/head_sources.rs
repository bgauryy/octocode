//! The head text that widens `matchString` hunks of a pull-request patch
//! read past GitHub's diff context.
use super::PrPlan;
use crate::tools::gh_get_history_item::HistoryItemRequest;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::inventory::file_page_size;
use crate::tools::gh_get_history_item::patch::hunks::{
    HeadSources, head_text_needed, match_context,
};
use crate::tools::gh_get_history_item::text::needle;
use crate::tools::gh_get_history_item::util::str_at;
use crate::tools::gh_get_history_item::window::Loaded;
use crate::tools::gh_shared::text::file_text;
use octocode_github::{ContentRequest, GitHubProvider, GitHubTransport, RequestContext};
use serde_json::Value;

/// Files read at the head per call to widen `matchString` context; the rest
/// keep their clip flag and `next.expandContext`.
const MAX_HEAD_SOURCES: usize = 10;

/// The head text of each in-scope file whose `matchString` hit runs need
/// more context than its hunks hold, read once at `headSha` (the history
/// read cache revalidates it). `None` when no file needs it.
pub(super) async fn head_sources(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    raw: &Value,
    loaded: &Loaded,
    plan: &PrPlan,
) -> Option<HeadSources> {
    let needle = needle(query)?;
    let sha = str_at(raw, "/head/sha").filter(|sha| !sha.is_empty())?;
    let lines = match_context(query);
    let filter = plan.file_filter();
    // The files this page shows (the loaded window holds the pages around
    // it): a later page widens its own hits, never the first page's again.
    let kept = loaded
        .items
        .iter()
        .filter(|file| filter.matches(file))
        .collect::<Vec<_>>();
    let (page_files, _) = loaded.state.paginate(
        kept,
        gh_get_history_item_query::page(query),
        Some(file_page_size(query, true)),
    );
    let paths: Vec<String> = page_files
        .iter()
        .filter(|file| str_at(file, "/status") != Some("removed"))
        .filter(|file| {
            str_at(file, "/patch").is_some_and(|patch| head_text_needed(patch, &needle, lines))
        })
        .filter_map(|file| str_at(file, "/filename").map(str::to_owned))
        .take(MAX_HEAD_SOURCES)
        .collect();
    if paths.is_empty() {
        return None;
    }
    // The one file read every GitHub tool shares (commit-pinned cache, single
    // flight, the blob fallback past 1 MB). A provider is the transport's
    // read API: this one shares the transport's cache, memos and flights.
    let provider = GitHubProvider {
        transport: transport.clone(),
    };
    let provider = &provider;
    let reads = paths.iter().map(|path| async move {
        let request = ContentRequest {
            owner: gh_get_history_item_query::owner(query).to_owned(),
            repo: gh_get_history_item_query::repo(query).to_owned(),
            path: path.clone(),
            reference: Some(sha.to_owned()),
            force_refresh: false,
        };
        let text = provider
            .get_file_bytes(&request, context)
            .await
            .ok()
            .and_then(|read| {
                file_text(&read.bytes)
                    .ok()
                    .map(|text| text.lines().map(str::to_owned).collect())
            });
        (path.clone(), text)
    });
    let mut sources = HeadSources::default();
    for (path, text) in futures_util::future::join_all(reads).await {
        sources.insert(path, text);
    }
    Some(sources)
}
