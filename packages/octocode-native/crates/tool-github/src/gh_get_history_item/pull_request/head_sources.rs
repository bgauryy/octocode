//! The head text that widens `matchString` hunks of a pull-request patch
//! read past GitHub's diff context.
use super::PrPlan;
use crate::gh_get_history_item::HistoryItemRequest;
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::inventory::file_page_size;
use crate::gh_get_history_item::patch::hunks::{HeadSources, head_text_needed, match_context};
use crate::gh_get_history_item::source::Source;
use crate::gh_get_history_item::text::needle;
use crate::gh_get_history_item::util::str_at;
use crate::gh_get_history_item::window::Loaded;
use octocode_github::{ContentRequest, GitHubProvider, RequestContext};
use octocode_tools_shared::gh_shared::text::file_text;
use serde_json::Value;

/// The head text of every file on this page whose `matchString` hit runs
/// need more context than its hunks hold, read at `headCommitSha`. The request
/// budget paces concurrent reads. `None` when no file needs it. A file the
/// path policy withholds is not read: it counts as unread, so its clipped runs
/// are flagged, and its patch alone stays (redacted). Each text read is
/// redacted with its own path, like the patch it widens, so no view numbers an
/// unredacted line; a text the scanner refuses counts as unread too.
pub(super) async fn head_sources(
    source: &Source<'_>,
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
        .collect();
    if paths.is_empty() {
        return None;
    }
    // The same file reader every GitHub tool uses, including the blob
    // fallback, shares this transport's connections and admission policy.
    let provider = GitHubProvider {
        transport: (**source).clone(),
    };
    let provider = &provider;
    let reads = paths.iter().map(|path| async move {
        if query.withholds_file(path) {
            return (path.clone(), None);
        }
        let request = ContentRequest {
            owner: gh_get_history_item_query::owner(query).to_owned(),
            repo: gh_get_history_item_query::repo(query).to_owned(),
            path: path.clone(),
            reference: Some(sha.to_owned()),
        };
        // A symlink's bytes are its target's, judged as a read of the
        // target would be: a withheld target is never asked for, and the
        // text is redacted by the target's name.
        let text = provider
            .get_file_bytes_following_links(&request, context, &|target| {
                query.withholds_file(target)
            })
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|(read, target)| {
                let mut text = file_text(&read.bytes).ok()?.into_owned();
                source
                    .redact(&mut text, target.as_deref().unwrap_or(path.as_str()))
                    .ok()?;
                Some(text.lines().map(str::to_owned).collect())
            });
        (path.clone(), text)
    });
    let mut sources = HeadSources::default();
    for (path, text) in futures_util::future::join_all(reads).await {
        sources.insert(path, text);
    }
    Some(sources)
}
