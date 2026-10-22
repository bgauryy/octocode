//! `operation: "pullRequest"`: concurrent collection loads (GraphQL first page
//! or REST windows), metadata row, and assembly of the shaped sections.
use super::axis::Axis;
use super::filter::{FileFilter, FileScope};
use super::graphql::{
    GraphqlCollection, GraphqlOutcome, GraphqlPr, graphql_complete_collection_eligible,
    graphql_pull_request, map_graphql_comments, map_graphql_commits, map_graphql_files,
    map_graphql_reviews,
};
use super::inventory::{file_page_size, shape_pr_files};
use super::patch::hunks::{HeadSources, head_text_needed, match_context};
use super::pr_finish::{finish_pr, pr_metadata, slim_row};
use super::pr_menu::{INVENTORY_ALL_PATCHES_FILES, pr_next_menu};
use super::pr_sections::{CommentSection, shape_pr_comments, shape_pr_commits, shape_pr_reviews};
use super::remaining::{BodyWindows, Remainder};
use super::text::{body_matches, needle, paginate_text};
use super::util::{map_comments, str_at};
use super::window::{
    Batch, Loaded, MAX_COLLECTION_BATCHES, MAX_FILE_BATCHES, MAX_PR_COMMIT_BATCHES, WindowSpec,
    load_window, reconcile_collection_total, reconcile_file_totals,
};
use super::{HistoryItemRequest, Section, Sections, fetch, validation};
use crate::num::usize_of;
use crate::response::result::ToolData;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use octocode_contract_types::ExtraKind;
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Map, Value, json};

/// One PR collection: what GraphQL served (the whole list, or a first page
/// it covers), otherwise the REST window covering the requested public page.
async fn load_collection(
    transport: &GitHubTransport,
    wanted: bool,
    graphql: Option<Loaded>,
    segments: &[&str],
    spec: WindowSpec,
    keep: impl Fn(&Value) -> bool,
    context: &RequestContext,
) -> Result<Option<Loaded>, ProviderError> {
    if !wanted {
        return Ok(None);
    }
    if let Some(served) = graphql {
        return Ok(Some(served));
    }
    load_window(transport, segments, spec, keep, context)
        .await
        .map(Some)
}

pub(super) async fn pull_request(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let wants = query.sections();
    let plan = PrPlan::new(query)?;
    // Review comments are REST-only: when GraphQL serves the rest of the
    // read, their first window loads beside it instead of after its answer.
    let ((graphql, graphql_fallback), inline) =
        if graphql_complete_collection_eligible(query) && wants.has(Section::ReviewComments) {
            let (fast, inline) = tokio::try_join!(
                fast_path(transport, query, context),
                load_inline(transport, query, context, &plan, None),
            )?;
            (fast, Some(inline))
        } else {
            (fast_path(transport, query, context).await?, None)
        };
    let loads = load_pr(
        transport,
        query,
        context,
        &wants,
        &plan,
        graphql.as_ref(),
        inline,
    )
    .await?;
    let head = str_at(&loads.raw, "/head/sha").map(str::to_owned);
    // A pin names the head by its SHA or an abbreviation of it.
    if let Some(pinned) = gh_get_history_item_query::head_sha(&query.query)
        && !head.as_deref().is_some_and(|head| head.starts_with(pinned))
    {
        // The head moved since the first page: nothing shown stays valid.
        return Ok(ToolData::from(crate::response::result::stale_snapshot(
            crate::response::result::restart_query(
                crate::id::ToolId::GhGetHistoryItem,
                &crate::response::result::query_row(&query.query, &[]),
            ),
        )));
    }
    let shaped = shape_pr(transport, query, context, &wants, &plan, loads).await?;
    let mut data = finish_pr(query, &wants, &plan, shaped, graphql_fallback);
    if let (Some(head), Some(number)) = (head, gh_get_history_item_query::number(query)) {
        pin_head_pages(&mut data.data, &head, number);
    }
    Ok(data)
}

/// Pins every page of this pull request (`next.*` of the row and of its
/// commits) to the head this read saw: a later page of a moved head is a
/// stale snapshot with `next.restart`, never a mix of two heads. Leads,
/// enrichments and the restart itself stay unpinned.
fn pin_head_pages(value: &mut Value, head: &str, number: u64) {
    match value {
        Value::Object(object) => {
            if let Some(Value::Object(next)) = object.get_mut("next") {
                let tool = crate::id::ToolId::GhGetHistoryItem;
                for (name, page) in next.iter_mut() {
                    if !crate::id::is_remaining(tool, name) || page["tool"] != tool.as_str() {
                        continue;
                    }
                    let rows = page
                        .pointer_mut("/query/queries")
                        .and_then(Value::as_array_mut)
                        .into_iter()
                        .flatten();
                    for row in rows {
                        if row["operation"] == "pullRequest" && row["number"] == number {
                            row["headSha"] = json!(head);
                        }
                    }
                }
            }
            for (key, child) in object.iter_mut() {
                if key != "next" && key != "hints" {
                    pin_head_pages(child, head, number);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                pin_head_pages(item, head, number);
            }
        }
        _ => {}
    }
}

/// The GraphQL fast path. REST serves whatever GraphQL could not; a failed
/// fast path keeps its reason, and a cancelled or expired request stops here.
async fn fast_path(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<(Option<GraphqlPr>, Option<String>), ProviderError> {
    if !graphql_complete_collection_eligible(query) {
        return Ok((None, None));
    }
    match graphql_pull_request(transport, query, context).await {
        Ok(GraphqlOutcome::Served(pr)) => Ok((Some(*pr), None)),
        Ok(GraphqlOutcome::Unavailable) => Ok((None, None)),
        Ok(GraphqlOutcome::Failed(reason)) => Ok((None, Some(reason))),
        Err(error) if error.is_cancelled_or_timed_out() => Err(error),
        Err(error) => Ok((None, Some(error.message.to_string()))),
    }
}

/// What a pull-request read selects: the literal and the file scope.
pub(super) struct PrPlan {
    number: String,
    pub(super) needle: Option<String>,
    pub(super) scope: Option<FileScope>,
}

impl PrPlan {
    fn new(query: &HistoryItemRequest) -> Result<Self, ProviderError> {
        let number = gh_get_history_item_query::number(query)
            .ok_or_else(|| validation("number is required"))?
            .to_string();
        let scope = FileScope::from_query(query).map_err(|message| validation(&message))?;
        Ok(Self {
            number,
            needle: needle(query),
            scope,
        })
    }
    fn file_filter(&self) -> FileFilter<'_> {
        FileFilter {
            needle: self.needle.as_deref(),
            scope: self.scope.as_ref(),
        }
    }
}

/// The provider data one pull-request read loaded.
struct PrLoads {
    raw: Value,
    files: Option<Loaded>,
    discussion: Option<Loaded>,
    inline: Option<Loaded>,
    reviews: Option<Loaded>,
    commits: Option<Loaded>,
}

/// Requests that run before the collections: the PR itself, beside the
/// first batch of a file window that may need more than one batch (the PR's
/// `changed_files` then sizes the rest, which load at once), or alone when a
/// later comment page that `matchString` filters needs the PR's counts.
async fn first_requests(
    transport: &GitHubTransport,
    context: &RequestContext,
    pulls: &[&str],
    file_batch: Option<usize>,
    later_comments: bool,
) -> Result<(Option<Value>, Option<(usize, Batch)>), ProviderError> {
    if let Some(batch) = file_batch {
        let files_path = [pulls, &["files"]].concat();
        let page = [
            ("per_page", octocode_github::MAX_PER_PAGE.to_string()),
            ("page", batch.to_string()),
        ];
        let ((raw, _), files) = tokio::try_join!(
            fetch(transport, pulls, &[], context),
            fetch(transport, &files_path, &page, context),
        )?;
        return Ok((Some(raw), Some((batch, files))));
    }
    if later_comments {
        return Ok((Some(fetch(transport, pulls, &[], context).await?.0), None));
    }
    Ok((None, None))
}

/// The PR fetched before its collections, beside the first batch of its
/// file window, when they need its counts: a selected or match-filtered file
/// scan cannot jump to a batch by index, a page whose files cross a batch
/// boundary needs the count to read its batches at once, and a later
/// comment page that `matchString` filters cannot jump either. The held
/// batch is the window's own first batch, never read again; when it ends
/// the list (most PRs), the read costs one round trip.
async fn prefetch(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &Sections,
    plan: &PrPlan,
    files: &WindowSpec,
    graphql: Option<&GraphqlPr>,
) -> Result<(Option<Value>, Option<(usize, Batch)>), ProviderError> {
    if graphql.is_some() {
        return Ok((None, None));
    }
    let file_batch = (wants.lists_files() && files.spans_batches()).then(|| files.first_batch());
    // Only a `matchString` filters comments; an unfiltered later page jumps
    // straight to its provider batch.
    let later_comments = (wants.has(Section::Comments) || wants.has(Section::ReviewComments))
        && plan.needle.is_some()
        && gh_get_history_item_query::page(query).is_some_and(|page| page > 1);
    let pulls = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "pulls",
        plan.number.as_str(),
    ];
    first_requests(transport, context, &pulls, file_batch, later_comments).await
}

/// The PR itself: from GraphQL, the prefetch, or one REST read.
async fn load_raw(
    transport: &GitHubTransport,
    context: &RequestContext,
    pulls: &[&str],
    graphql: Option<&GraphqlPr>,
    prefetched: Option<Value>,
) -> Result<Value, ProviderError> {
    match (graphql, prefetched) {
        (Some(graphql), _) => Ok(graphql.raw.clone()),
        (None, Some(raw)) => Ok(raw),
        (None, None) => fetch(transport, pulls, &[], context)
            .await
            .map(|(raw, _)| raw),
    }
}

/// A collection's item count on the prefetched PR (`changed_files`,
/// `comments`, `review_comments`).
fn provider_count(raw: Option<&Value>, key: &str) -> Option<usize> {
    raw.and_then(|raw| raw.get(key))
        .and_then(Value::as_u64)
        .map(usize_of)
}

/// The review-comment (inline) window of the requested page, sized by the
/// PR's `review_comments` count when it is known.
async fn load_inline(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    plan: &PrPlan,
    provider_total: Option<usize>,
) -> Result<Option<Loaded>, ProviderError> {
    let needle = plan.needle.as_deref();
    let path = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "pulls",
        plan.number.as_str(),
        "comments",
    ];
    load_collection(
        transport,
        true,
        None,
        &path,
        WindowSpec {
            max_batches: MAX_COLLECTION_BATCHES,
            page: gh_get_history_item_query::page(query).unwrap_or(1),
            page_size: gh_get_history_item_query::collection_page_size(query),
            filtered: needle.is_some(),
            provider_total,
            prefetched: None,
        },
        |value| body_matches(value, needle),
        context,
    )
    .await
}

/// `inline` is the review-comment window when it already loaded beside
/// the GraphQL read.
async fn load_pr(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &Sections,
    plan: &PrPlan,
    graphql: Option<&GraphqlPr>,
    inline: Option<Option<Loaded>>,
) -> Result<PrLoads, ProviderError> {
    let (file_filter, needle) = (plan.file_filter(), plan.needle.as_deref());
    let page_size = gh_get_history_item_query::collection_page_size(query);
    let comment_filter = |value: &Value| body_matches(value, needle);
    let spec = |max_batches, page: Option<usize>, filtered| WindowSpec {
        max_batches,
        page: page.unwrap_or(1),
        page_size,
        filtered,
        provider_total: None,
        prefetched: None,
    };
    let (owner, repo, number) = (
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        plan.number.as_str(),
    );
    let under_pulls = |leaf: &'static str| ["repos", owner, repo, "pulls", number, leaf];
    let [files_path, reviews_path, commits_path] = ["files", "reviews", "commits"].map(under_pulls);
    let discussion_path = ["repos", owner, repo, "issues", number, "comments"];
    let complete = |kind: fn(&GraphqlPr) -> GraphqlCollection, map: fn(&Value) -> Vec<Value>| {
        graphql.and_then(|g| match kind(g) {
            GraphqlCollection::Complete => Some(Loaded::complete(map(&g.source))),
            GraphqlCollection::Partial => Some(Loaded::prefix(map(&g.source))),
            GraphqlCollection::Unused | GraphqlCollection::Incomplete => None,
        })
    };
    let mut files = WindowSpec {
        page_size: file_page_size(query, wants.has(Section::Patches)),
        ..spec(
            MAX_FILE_BATCHES,
            gh_get_history_item_query::page(query),
            !file_filter.is_trivial(),
        )
    };
    let (raw_first, file_batch) =
        prefetch(transport, query, context, wants, plan, &files, graphql).await?;
    // The PR's counts size the collections' batches: from the prefetch, or
    // from the GraphQL header (which counts files and comments).
    let counts = raw_first.as_ref().or(graphql.map(|g| &g.raw));
    let provider_count = |key: &str| provider_count(counts, key);
    files.provider_total = provider_count("changed_files");
    files.prefetched = file_batch;
    let pulls = ["repos", owner, repo, "pulls", number];
    // Collections load concurrently; each derives its provider batches from
    // the public page cursor (no provider cursor leaks into continuations).
    let (raw, files, discussion, inline, reviews, commits) = tokio::try_join!(
        load_raw(transport, context, &pulls, graphql, raw_first.clone()),
        load_collection(
            transport,
            wants.lists_files(),
            complete(|g| g.files, map_graphql_files),
            &files_path,
            files,
            |value| file_filter.matches(value),
            context,
        ),
        load_collection(
            transport,
            wants.has(Section::Comments),
            complete(|g| g.discussion, map_graphql_comments),
            &discussion_path,
            WindowSpec {
                provider_total: provider_count("comments"),
                ..spec(
                    MAX_COLLECTION_BATCHES,
                    gh_get_history_item_query::page(query),
                    needle.is_some()
                )
            },
            comment_filter,
            context,
        ),
        async {
            match inline {
                Some(loaded) => Ok(loaded),
                None if wants.has(Section::ReviewComments) => {
                    let total = provider_count("review_comments");
                    load_inline(transport, query, context, plan, total).await
                }
                None => Ok(None),
            }
        },
        load_collection(
            transport,
            wants.has(Section::Reviews),
            complete(|g| g.reviews, map_graphql_reviews),
            &reviews_path,
            spec(
                MAX_COLLECTION_BATCHES,
                gh_get_history_item_query::page(query),
                needle.is_some()
            ),
            comment_filter,
            context,
        ),
        load_collection(
            transport,
            wants.lists_commits(),
            complete(|g| g.commits, map_graphql_commits),
            &commits_path,
            spec(
                MAX_PR_COMMIT_BATCHES,
                gh_get_history_item_query::page(query),
                false
            ),
            |_| true,
            context,
        ),
    )?;
    Ok(PrLoads {
        raw,
        files,
        discussion,
        inline,
        reviews,
        commits,
    })
}

/// What the changed-file section of a read found.
#[derive(Default)]
pub(super) struct FileOutcome {
    review: Vec<String>,
    pub(super) unsearched: Vec<String>,
    /// The first unsearched path and the commit holding its text (the
    /// base for a removed file, else the head).
    pub(super) first_unsearched: Option<(String, Option<String>)>,
    /// `(dropped, listed)` changed files of the `matchString` filter.
    pub(super) match_dropped: Option<(usize, usize)>,
    pub(super) scope_unmatched: bool,
    /// Changed files the read's `include`/`status`/`minChanges` scope
    /// kept (every listed file without one), when the scope or literal
    /// matched nothing.
    pub(super) in_scope: usize,
    pub(super) merge_read: Option<Value>,
    /// `readAtCommit`/`readParent`: the top file at the head (when no merge
    /// read covers it) and at the base.
    pub(super) side_reads: Vec<(&'static str, Value)>,
    /// Files whose `matchString` context the hunks cut short.
    pub(super) clipped: Vec<(String, Vec<String>)>,
    /// Files of the patch window not fully delivered.
    unfinished_patches: usize,
}

fn shape_files_section(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    loaded: Loaded,
    raw: &Value,
    query: &HistoryItemRequest,
    wants: &Sections,
    plan: &PrPlan,
) -> FileOutcome {
    let file_filter = plan.file_filter();
    // A cross-tool read: one check at the merge commit, on the first page,
    // over every loaded file (a later patch window does not repeat it, and
    // the shown window may hold no code file).
    let merge_read = (!gh_get_history_item_query::later_page(query))
        .then(|| {
            super::pr_menu::read_at_merge(query, raw, &loaded.items, |file| {
                file_filter.matches(file)
            })
        })
        .flatten();
    // Both sides of the numbered diff for its top file: the head (a merged
    // PR's merge read stands for its new side) and the base, whose old-side
    // numbers match while the base has not moved past the diff's merge base.
    // A file GitHub sent without a patch comes first: no merge read covers
    // it, so its head read stays.
    let side_reads = if gh_get_history_item_query::later_page(query) || !wants.has(Section::Patches)
    {
        Vec::new()
    } else {
        let unpatched =
            super::pr_menu::first_unpatched(&loaded.items, |file| file_filter.matches(file))
                .is_some();
        super::pr_menu::change_reads(
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            &loaded.items,
            &super::pr_menu::ChangeSides {
                new_ref: str_at(raw, "/head/sha").filter(|_| merge_read.is_none() || unpatched),
                old_ref: str_at(raw, "/base/sha").filter(|sha| !sha.is_empty()),
                old_confidence: crate::response::result::Confidence::Medium,
            },
            |file| file_filter.matches(file),
        )
    };
    let listed = loaded.state.skipped + loaded.items.len();
    let state = loaded.state;
    // An `include`/`status`/`minChanges` scope that matched no changed file at all.
    let scope_unmatched = (plan.scope.is_some() || plan.needle.is_some())
        && state.exhausted
        && gh_get_history_item_query::page(query).unwrap_or(1) == 1
        && !loaded.items.iter().any(|file| file_filter.matches(file));
    let in_scope = if scope_unmatched {
        loaded
            .items
            .iter()
            .filter(|file| plan.scope.as_ref().is_none_or(|scope| scope.matches(file)))
            .count()
    } else {
        0
    };
    let shaped = shape_pr_files(
        row,
        pagination,
        loaded.items,
        state,
        query,
        wants.has(Section::Patches),
        plan.scope.as_ref(),
    );
    if let Some(page) = pagination.get_mut("files") {
        let changed_files = raw
            .get("changed_files")
            .and_then(Value::as_u64)
            .map(usize_of);
        reconcile_file_totals(
            page,
            state,
            listed,
            changed_files,
            !file_filter.is_trivial(),
        );
    }
    FileOutcome {
        review: shaped.review,
        unsearched: shaped.unsearched,
        first_unsearched: shaped.first_unsearched.map(|(path, removed)| {
            let side = if removed { "/base/sha" } else { "/head/sha" };
            let sha = str_at(raw, side).filter(|sha| !sha.is_empty());
            (path, sha.map(str::to_owned))
        }),
        match_dropped: shaped.match_dropped,
        scope_unmatched,
        in_scope,
        merge_read,
        side_reads,
        clipped: shaped.clipped,
        unfinished_patches: shaped.unfinished_patches,
    }
}

/// Files read at the head per call to widen `matchString` context; the rest
/// keep their clip flag and `next.expandContext`.
const MAX_HEAD_SOURCES: usize = 10;

/// The head text of each in-scope file whose `matchString` hit runs need
/// more context than its hunks hold, read once at `headSha` (the history
/// read cache revalidates it). `None` when no file needs it.
async fn head_sources(
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
    let reads = paths.iter().map(|path| async move {
        let mut segments = vec![
            "repos",
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            "contents",
        ];
        segments.extend(path.split('/'));
        let text = fetch(transport, &segments, &[("ref", sha.to_owned())], context)
            .await
            .ok()
            .and_then(|(value, _)| contents_text(value));
        (path.clone(), text)
    });
    let mut sources = HeadSources::default();
    for (path, text) in futures_util::future::join_all(reads).await {
        sources.insert(
            path,
            text.map(|text| text.lines().map(str::to_owned).collect()),
        );
    }
    Some(sources)
}

/// A Contents API file body as text (the content decoder file reads use);
/// `None` for a directory, a body GitHub left out (over 1 MB), or bytes
/// that are not UTF-8.
fn contents_text(mut value: Value) -> Option<String> {
    let content = value.get_mut("content").map(Value::take)?;
    let bytes = octocode_github::decode_content(
        value.get("encoding").and_then(Value::as_str),
        content.as_str().map(str::to_owned),
    )
    .ok()?;
    String::from_utf8(bytes).ok()
}

/// The read's own next steps. A summary offers the PR-read menu; an
/// unfiltered first inventory page offers its review pick (and every patch
/// on a small PR); a filtered inventory or a patch read is a targeted answer
/// and keeps only its continuations.
fn read_menu(
    query: &HistoryItemRequest,
    review: &[String],
    raw: &Value,
    slim: bool,
    file_read: bool,
) -> Option<Value> {
    if !slim {
        return Some(pr_next_menu(query, review, raw));
    }
    let first_inventory = file_read
        && !query.sections().has(Section::Patches)
        && !gh_get_history_item_query::later_page(query)
        && !gh_get_history_item_query::has_file_filter(query);
    if !first_inventory {
        return None;
    }
    let all_patches = raw
        .get("changed_files")
        .and_then(Value::as_u64)
        .is_some_and(|files| files <= INVENTORY_ALL_PATCHES_FILES);
    let mut next = pr_next_menu(query, review, raw);
    if let Some(menu) = next.as_object_mut() {
        menu.retain(|name, _| {
            name == "readSelectedPatches" || (all_patches && name == "readPatches")
        });
    }
    Some(next).filter(|next| next.as_object().is_some_and(|next| !next.is_empty()))
}

/// A shaped pull-request row and what the response still needs from it.
pub(super) struct ShapedPr {
    pub(super) row: Value,
    pub(super) files: FileOutcome,
    pub(super) menu: Option<Value>,
    /// The code the page's anchored review comments discuss, ten ranges a read.
    pub(super) comment_reads: Vec<Value>,
    /// What the row's pages leave to read beside its public pages.
    pub(super) remainder: Remainder,
    /// Each listed commit's next file page (`continueCommitFiles…`).
    pub(super) commit_file_pages: Vec<(String, Value)>,
}

async fn shape_pr(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &Sections,
    plan: &PrPlan,
    loads: PrLoads,
) -> Result<ShapedPr, ProviderError> {
    let PrLoads {
        raw,
        files,
        discussion,
        inline,
        reviews,
        commits,
    } = loads;
    let patches = wants.has(Section::Patches);
    let slim = (gh_get_history_item_query::later_page(query)
        || gh_get_history_item_query::has_sections(query))
        && !gh_get_history_item_query::wants(query, ExtraKind::Details);
    let mut row = pr_metadata(&raw, query);
    if slim {
        slim_row(&mut row, query);
    }
    let mut pagination = Map::new();
    let mut remainder = Remainder::default();
    if wants.has(Section::Body) {
        shape_body(&mut row, &mut pagination, &raw, query);
    }
    let widened;
    let query = match &files {
        Some(loaded) if patches => {
            match head_sources(transport, query, context, &raw, loaded, plan).await {
                Some(sources) => {
                    widened = HistoryItemRequest {
                        head_sources: Some(std::sync::Arc::new(sources)),
                        ..query.clone()
                    };
                    &widened
                }
                None => query,
            }
        }
        _ => query,
    };
    let file_outcome = files.map_or_else(FileOutcome::default, |loaded| {
        shape_files_section(&mut row, &mut pagination, loaded, &raw, query, wants, plan)
    });
    remainder.unfinished_patches(file_outcome.unfinished_patches);
    let comment_reads = shape_comment_sections(
        &mut row,
        &mut pagination,
        &mut remainder,
        inline,
        discussion,
        query,
    );
    let mut review_bodies = BodyWindows::default();
    if let Some(loaded) = reviews {
        shape_pr_reviews(
            &mut row,
            &mut pagination,
            loaded.items,
            loaded.state,
            query,
            &mut review_bodies,
        );
    }
    remainder.bodies(Axis::ReviewBody, review_bodies, &mut pagination);
    let mut commit_file_pages = Vec::new();
    if let Some(loaded) = commits {
        let listed = loaded.state.skipped.saturating_add(loaded.items.len());
        let state = loaded.state;
        commit_file_pages = shape_pr_commits(
            transport,
            &mut row,
            &mut pagination,
            loaded.items,
            loaded.state,
            query,
            context,
        )
        .await?;
        let total = raw
            .get("commits")
            .and_then(Value::as_u64)
            .and_then(|total| usize::try_from(total).ok());
        if let Some(page) = pagination.get_mut("commits")
            && reconcile_collection_total(page, state, listed, total, "providerCommitListLimit")
        {
            // The slim section header must keep the authoritative denominator
            // when the provider's returned collection cannot reach it.
            row["commitsCount"] = raw["commits"].clone();
        }
    }
    let menu = read_menu(query, &file_outcome.review, &raw, slim, wants.lists_files());
    if slim && patches {
        drop_named_header(&mut row);
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    Ok(ShapedPr {
        row,
        files: file_outcome,
        menu,
        comment_reads,
        remainder,
        commit_file_pages,
    })
}

/// The body's verbatim text window on `row` and its page.
fn shape_body(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    raw: &Value,
    query: &HistoryItemRequest,
) {
    let body = raw.get("body").and_then(Value::as_str).unwrap_or("");
    let (text, page) = paginate_text(
        body,
        gh_get_history_item_query::char_offset(query),
        gh_get_history_item_query::char_length(query),
    );
    row["body"] = json!(text);
    pagination.insert(Axis::Body.key().into(), page);
}

/// Patch rows are the evidence of a section patch read: it re-proves the
/// PR's name (number, title, author), state, and the head it read. The
/// metadata read dates the PR (createdAt); a read without patch rows keeps
/// it.
fn drop_named_header(row: &mut Value) {
    let patch_rows = row["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty() && files.iter().all(Value::is_object));
    if patch_rows
        && row.get("headSha").is_some()
        && let Some(fields) = row.as_object_mut()
    {
        fields.remove("createdAt");
    }
}

/// The inline review comments (`reviewComments`) and the discussion
/// (`comments`), each one page of its own axis; their cut bodies share the
/// one `commentBody` window. Returns the review comments' code reads.
fn shape_comment_sections(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    remainder: &mut Remainder,
    inline: Option<Loaded>,
    discussion: Option<Loaded>,
    query: &HistoryItemRequest,
) -> Vec<Value> {
    let mut bodies = BodyWindows::default();
    let mut reads = Vec::new();
    for (section, loaded) in [
        (CommentSection::ReviewInline, inline),
        (CommentSection::Discussion, discussion),
    ] {
        let Some(loaded) = loaded else { continue };
        reads.extend(shape_pr_comments(
            row,
            pagination,
            section,
            map_comments(loaded.items),
            loaded.state,
            query,
            &mut bodies,
        ));
    }
    remainder.bodies(Axis::CommentBody, bodies, pagination);
    reads
}
