//! `operation: "pullRequest"`: concurrent collection loads (GraphQL first page
//! or REST windows), metadata row, and assembly of the shaped sections.
use super::filter::{FileFilter, InventoryFilter, patch_selection};
use super::graphql::{
    GraphqlCollection, GraphqlOutcome, GraphqlPr, graphql_complete_collection_eligible,
    graphql_pull_request, map_graphql_comments, map_graphql_commits, map_graphql_files,
    map_graphql_reviews,
};
use super::inventory::{file_page_size, shape_pr_files};
use super::patch::{HeadSources, clamp_warning, head_text_needed, match_context, push_warning};
use super::patch_hop::attach_full_patch_continuation;
use super::pr_menu::{INVENTORY_ALL_PATCHES_FILES, pr_next_menu};
use super::pr_sections::{shape_pr_comments, shape_pr_commits, shape_pr_reviews};
use super::promotion::{attach_raw_body_read, promote_pr_continuations};
use super::util::{
    body_matches, content_flag, history_body_view, is_bot, map_comments, needle, nonzero,
    paginate_text, str_at, string, view_dropped_text,
};
use super::window::{
    Loaded, MAX_COLLECTION_BATCHES, MAX_FILE_BATCHES, MAX_PR_COMMIT_BATCHES, PROVIDER_BATCH,
    WindowSpec, WindowState, load_window, reconcile_file_totals,
};
use super::{HistoryItemRequest, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use serde_json::{Map, Value, json};

/// The content sections a pull-request query asks for.
pub(super) struct ContentWants {
    pub(super) body: bool,
    pub(super) files: bool,
    pub(super) discussion: bool,
    inline: bool,
    pub(super) reviews: bool,
    pub(super) commits: bool,
    include_bots: bool,
    pub(super) patch_mode: String,
}

pub(super) fn content_wants(query: &HistoryItemRequest) -> ContentWants {
    let content_value = query.content_value();
    let content = content_value.as_ref().and_then(Value::as_object);
    let patch_mode = content
        .and_then(|c| c.get("patches"))
        .and_then(|p| p.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_owned();
    let comments_selector = content
        .and_then(|c| c.get("comments"))
        .and_then(Value::as_object);
    ContentWants {
        body: content_flag(content, "body"),
        files: content_flag(content, "files") || patch_mode != "none",
        discussion: content_flag(comments_selector, "discussion"),
        inline: content_flag(comments_selector, "reviewInline"),
        reviews: content_flag(content, "reviews"),
        commits: content
            .and_then(|c| c.get("commits"))
            .and_then(Value::as_object)
            .is_some(),
        include_bots: content_flag(comments_selector, "includeBots"),
        patch_mode,
    }
}

/// One PR collection: the whole list when GraphQL returned it complete,
/// otherwise the REST window covering the requested public page.
async fn load_collection<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    wanted: bool,
    graphql: Option<Vec<Value>>,
    segments: &[&str],
    spec: WindowSpec,
    keep: impl Fn(&Value) -> bool,
    context: &RequestContext,
) -> Result<Option<Loaded>, ProviderError> {
    if !wanted {
        return Ok(None);
    }
    if let Some(items) = graphql {
        return Ok(Some(Loaded::complete(items)));
    }
    load_window(transport, segments, spec, keep, context)
        .await
        .map(Some)
}

pub(super) async fn pull_request<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let wants = content_wants(query);
    let (graphql, graphql_fallback) = fast_path(transport, query, context, &wants).await?;
    let plan = PrPlan::new(query)?;
    let loads = load_pr(transport, query, context, &wants, &plan, graphql.as_ref()).await?;
    let shaped = shape_pr(transport, query, context, &wants, &plan, loads).await?;
    Ok(finish_pr(query, &wants, &plan, shaped, graphql_fallback))
}

/// The GraphQL fast path. REST serves whatever GraphQL could not; a failed
/// fast path keeps its reason, and a cancelled or expired request stops here.
async fn fast_path<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &ContentWants,
) -> Result<(Option<GraphqlPr>, Option<String>), ProviderError> {
    if !graphql_complete_collection_eligible(query) {
        return Ok((None, None));
    }
    match graphql_pull_request(transport, query, context, wants).await {
        Ok(GraphqlOutcome::Served(pr)) => Ok((Some(*pr), None)),
        Ok(GraphqlOutcome::Unavailable) => Ok((None, None)),
        Ok(GraphqlOutcome::Failed(reason)) => Ok((None, Some(reason))),
        Err(error)
            if matches!(
                error.kind,
                ProviderErrorKind::Cancelled | ProviderErrorKind::Timeout
            ) =>
        {
            Err(error)
        }
        Err(error) => Ok((None, Some(error.message.to_string()))),
    }
}

/// What a pull-request read selects: the content selector, the patch
/// selection, the literal and the file scope.
struct PrPlan {
    number: String,
    content: Option<Value>,
    selection: (Vec<String>, super::filter::PatchRanges),
    needle: Option<String>,
    scope: Option<InventoryFilter>,
}

impl PrPlan {
    fn new(query: &HistoryItemRequest) -> Result<Self, ProviderError> {
        let number = query
            .number()
            .ok_or_else(|| validation("number is required"))?
            .to_string();
        let content = query.content_value();
        let selection = patch_selection(
            content
                .as_ref()
                .and_then(|c| c.get("patches"))
                .and_then(Value::as_object),
        );
        let scope = InventoryFilter::from_query(query).map_err(|message| validation(&message))?;
        Ok(Self {
            number,
            content,
            selection,
            needle: needle(query),
            scope,
        })
    }
    fn content(&self) -> Option<&Map<String, Value>> {
        self.content.as_ref().and_then(Value::as_object)
    }
    fn patch_selector(&self) -> Option<&Map<String, Value>> {
        self.content()
            .and_then(|c| c.get("patches"))
            .and_then(Value::as_object)
    }
    fn file_filter(&self) -> FileFilter<'_> {
        FileFilter {
            selected: &self.selection.0,
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
/// first file batch, when a filtered file scan or a later comment page needs
/// the PR's counts to read every batch at once.
async fn first_requests<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    context: &RequestContext,
    pulls: &[&str],
    filtered_files: bool,
    later_comments: bool,
) -> Result<(Option<Value>, Option<Vec<Value>>), ProviderError> {
    if filtered_files {
        let files_path = [pulls, &["files"]].concat();
        let batch = [
            ("per_page", PROVIDER_BATCH.to_string()),
            ("page", "1".to_owned()),
        ];
        let (raw, (files, more)) = tokio::try_join!(
            fetch(transport, pulls, &[], context),
            fetch(transport, &files_path, &batch, context),
        )?;
        let whole = (!more).then(|| files.as_array().cloned()).flatten();
        return Ok((Some(raw.0), whole));
    }
    if later_comments {
        return Ok((Some(fetch(transport, pulls, &[], context).await?.0), None));
    }
    Ok((None, None))
}

/// The PR fetched before its collections, beside its first file batch, when
/// they need its counts: a selected or match-filtered file scan cannot jump
/// to a batch by index, nor can a later comment page that filters bots; with
/// the PR's counts in hand they read every batch at once. When the first
/// file batch is the whole list (most PRs), a filtered read costs one round
/// trip.
async fn prefetch<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &ContentWants,
    plan: &PrPlan,
    graphql: Option<&GraphqlPr>,
) -> Result<(Option<Value>, Option<Vec<Value>>), ProviderError> {
    if graphql.is_some() {
        return Ok((None, None));
    }
    let filtered_files = wants.files && !plan.file_filter().is_trivial();
    let later_comments =
        (wants.discussion || wants.inline) && query.comment_page().is_some_and(|page| page > 1);
    let pulls = [
        "repos",
        query.owner(),
        query.repo(),
        "pulls",
        plan.number.as_str(),
    ];
    first_requests(transport, context, &pulls, filtered_files, later_comments).await
}

/// The PR itself: from GraphQL, the prefetch, or one REST read.
async fn load_raw<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
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
        .map(|total| usize::try_from(total).unwrap_or(usize::MAX))
}

async fn load_pr<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &ContentWants,
    plan: &PrPlan,
    graphql: Option<&GraphqlPr>,
) -> Result<PrLoads, ProviderError> {
    let (file_filter, needle) = (plan.file_filter(), plan.needle.as_deref());
    let (include_bots, page_size) = (wants.include_bots, query.collection_page_size());
    let body_filter = |value: &Value| body_matches(value, needle);
    let comment_filter = |value: &Value| {
        (include_bots || !is_bot(str_at(value, "/user/login").unwrap_or(""))) && body_filter(value)
    };
    let spec = |max_batches, page: Option<usize>, filtered| WindowSpec {
        max_batches,
        page: page.unwrap_or(1),
        page_size,
        filtered,
        provider_total: None,
    };
    let (owner, repo, number) = (query.owner(), query.repo(), plan.number.as_str());
    let under_pulls = |leaf: &'static str| ["repos", owner, repo, "pulls", number, leaf];
    let [files_path, inline_path, reviews_path, commits_path] =
        ["files", "comments", "reviews", "commits"].map(under_pulls);
    let discussion_path = ["repos", owner, repo, "issues", number, "comments"];
    let complete = |kind: fn(&GraphqlPr) -> GraphqlCollection, map: fn(&Value) -> Vec<Value>| {
        graphql
            .filter(|g| kind(g) == GraphqlCollection::Complete)
            .map(|g| map(&g.source))
    };
    let (raw_first, first_file_batch) =
        prefetch(transport, query, context, wants, plan, graphql).await?;
    let provider_count = |key: &str| provider_count(raw_first.as_ref(), key);
    let pulls = ["repos", owner, repo, "pulls", number];
    // Collections load concurrently; each derives its provider batches from
    // the public page cursor (no provider cursor leaks into continuations).
    let (raw, files, discussion, inline, reviews, commits) = tokio::try_join!(
        load_raw(transport, context, &pulls, graphql, raw_first.clone()),
        load_collection(
            transport,
            wants.files,
            complete(|g| g.files, map_graphql_files).or(first_file_batch),
            &files_path,
            WindowSpec {
                provider_total: provider_count("changed_files"),
                page_size: file_page_size(query, wants.patch_mode != "none"),
                ..spec(
                    MAX_FILE_BATCHES,
                    query.file_page(),
                    !file_filter.is_trivial()
                )
            },
            |value| file_filter.matches(value),
            context,
        ),
        load_collection(
            transport,
            wants.discussion,
            complete(|g| g.discussion, map_graphql_comments),
            &discussion_path,
            WindowSpec {
                provider_total: provider_count("comments"),
                ..spec(MAX_COLLECTION_BATCHES, query.comment_page(), true)
            },
            comment_filter,
            context,
        ),
        load_collection(
            transport,
            wants.inline,
            None,
            &inline_path,
            WindowSpec {
                provider_total: provider_count("review_comments"),
                ..spec(MAX_COLLECTION_BATCHES, query.comment_page(), true)
            },
            comment_filter,
            context,
        ),
        load_collection(
            transport,
            wants.reviews,
            complete(|g| g.reviews, map_graphql_reviews),
            &reviews_path,
            spec(
                MAX_COLLECTION_BATCHES,
                query.review_page(),
                needle.is_some()
            ),
            body_filter,
            context,
        ),
        load_collection(
            transport,
            wants.commits,
            complete(|g| g.commits, map_graphql_commits),
            &commits_path,
            spec(MAX_PR_COMMIT_BATCHES, query.commit_page(), false),
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

/// Inline review comments sort before discussion; the combined list is
/// complete only when both sources are. Hidden bots are counted.
fn merge_comments(
    inline: Option<Loaded>,
    discussion: Option<Loaded>,
    include_bots: bool,
) -> (Vec<Value>, Option<WindowState>, Vec<String>) {
    let mut comments = Vec::new();
    let mut state: Option<WindowState> = None;
    let mut warnings = Vec::new();
    for (loaded, kind, label) in [
        (inline, "review_inline", "inline "),
        (discussion, "discussion", ""),
    ] {
        let Some(loaded) = loaded else { continue };
        state = Some(match state {
            None => loaded.state,
            Some(previous) => previous.merge(loaded.state),
        });
        let dropped = loaded
            .items
            .iter()
            .filter(|v| is_bot(str_at(v, "/user/login").unwrap_or("")))
            .count();
        comments.extend(map_comments(loaded.items, kind, include_bots));
        if !include_bots && dropped > 0 {
            warnings.push(format!(
                "{dropped} bot {label}comment(s) hidden (set includeBots:true to include)"
            ));
        }
    }
    (comments, state, warnings)
}

/// A later page already holds the header and the menu from page one, and a
/// file inventory or patch read is read for its files: both carry only the
/// identity fields every page must re-prove, plus the fields the output
/// contract requires of every pull-request row. Every view keeps the three
/// commits that name the change (`sourceSha`, `targetSha`, and a merged
/// PR's `mergeCommitSha`); a list page also keeps the file count, and a
/// first inventory page the diff size it lists. Merge state rides every
/// row; labels ride the first page.
fn slim_row(row: &mut Value, query: &HistoryItemRequest, patches: bool) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    let first_page = !query.later_page();
    let totals = first_page && !patches;
    fields.retain(|key, _| {
        matches!(
            key.as_str(),
            "number"
                | "title"
                | "state"
                | "author"
                | "createdAt"
                | "sourceSha"
                | "targetSha"
                | "mergeCommitSha"
                | "mergedAt"
                | "closedAt"
                | "targetBranch"
        ) || (first_page && key == "labels")
            || (!patches && key == "changedFilesCount")
            || (totals && matches!(key.as_str(), "additions" | "deletions"))
    });
}

/// What the changed-file section of a read found.
#[derive(Default)]
struct FileOutcome {
    no_selected_files_matched: bool,
    review: Vec<String>,
    unsearched: Vec<String>,
    first_unsearched: Option<String>,
    scope_unmatched: bool,
    merge_read: Option<Value>,
    /// `readAtCommit`/`readParent`: the top file at the head (when no merge
    /// read covers it) and at the base.
    side_reads: Vec<(&'static str, Value)>,
    /// Files whose `matchString` context the hunks cut short.
    clipped: Vec<(String, Vec<String>)>,
}

fn shape_files_section(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    loaded: Loaded,
    raw: &Value,
    query: &HistoryItemRequest,
    wants: &ContentWants,
    plan: &PrPlan,
) -> FileOutcome {
    let file_filter = plan.file_filter();
    // A cross-tool read: one check at the merge commit, on the first page,
    // over every loaded file (a later patch window does not repeat it, and
    // the shown window may hold no code file).
    let merge_read = (!query.later_page())
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
    let side_reads = if query.later_page() || wants.patch_mode == "none" {
        Vec::new()
    } else {
        let unpatched =
            super::pr_menu::first_unpatched(&loaded.items, |file| file_filter.matches(file))
                .is_some();
        super::pr_menu::change_reads(
            query.owner(),
            query.repo(),
            &loaded.items,
            &super::pr_menu::ChangeSides {
                new_ref: str_at(raw, "/head/sha").filter(|_| merge_read.is_none() || unpatched),
                old_ref: str_at(raw, "/base/sha").filter(|sha| !sha.is_empty()),
                old_confidence: "medium",
            },
            |file| file_filter.matches(file),
        )
    };
    let listed = loaded.state.skipped + loaded.items.len();
    let state = loaded.state;
    // An `include`/`status`/`minChanges` scope that matched no changed file at all.
    let scope_unmatched = (plan.scope.is_some() || plan.needle.is_some())
        && state.exhausted
        && query.file_page().unwrap_or(1) == 1
        && !loaded.items.iter().any(|file| file_filter.matches(file));
    let shaped = shape_pr_files(
        row,
        pagination,
        loaded.items,
        state,
        query,
        plan.patch_selector(),
        &wants.patch_mode,
        plan.scope.as_ref(),
    );
    if let Some(page) = pagination.get_mut("files") {
        let changed_files = raw
            .get("changed_files")
            .and_then(Value::as_u64)
            .map(|total| usize::try_from(total).unwrap_or(usize::MAX));
        reconcile_file_totals(
            page,
            state,
            listed,
            changed_files,
            !file_filter.is_trivial(),
        );
    }
    FileOutcome {
        no_selected_files_matched: shaped.no_selected_match,
        review: shaped.review,
        unsearched: shaped.unsearched,
        first_unsearched: shaped.first_unsearched,
        scope_unmatched,
        merge_read,
        side_reads,
        clipped: shaped.clipped,
    }
}

/// Files read at the head per call to widen `matchString` context; the rest
/// keep their clip flag and `next.expandContext`.
const MAX_HEAD_SOURCES: usize = 10;

/// The head text of each in-scope file whose `matchString` hit runs need
/// more context than its hunks hold, read once at `sourceSha` (the history
/// read cache revalidates it). `None` when no file needs it.
async fn head_sources<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
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
    let paths: Vec<&str> = loaded
        .items
        .iter()
        .filter(|file| filter.matches(file) && str_at(file, "/status") != Some("removed"))
        .filter(|file| {
            str_at(file, "/patch").is_some_and(|patch| head_text_needed(patch, &needle, lines))
        })
        .filter_map(|file| str_at(file, "/filename"))
        .take(MAX_HEAD_SOURCES)
        .collect();
    if paths.is_empty() {
        return None;
    }
    let reads = paths.iter().map(|path| async move {
        let mut segments = vec!["repos", query.owner(), query.repo(), "contents"];
        segments.extend(path.split('/'));
        let text = fetch(transport, &segments, &[("ref", sha.to_owned())], context)
            .await
            .ok()
            .and_then(|(value, _)| decode_contents(&value));
        ((*path).to_owned(), text)
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

/// A Contents API file body as text; `None` for a directory, a body GitHub
/// left out (over 1 MB), or bytes that are not UTF-8.
fn decode_contents(value: &Value) -> Option<String> {
    use base64::Engine as _;
    if str_at(value, "/encoding") != Some("base64") {
        return None;
    }
    let packed: String = str_at(value, "/content")?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(packed)
        .ok()?;
    String::from_utf8(bytes).ok()
}

/// The read's own next steps. A summary offers the PR-read menu; an
/// unfiltered first inventory page offers its review pick (and every patch
/// on a small PR); a filtered inventory or a patch read is a targeted answer
/// and keeps only its continuations.
fn read_menu(
    query: &HistoryItemRequest,
    plan: &PrPlan,
    patch_mode: &str,
    review: &[String],
    raw: &Value,
    slim: bool,
    file_read: bool,
) -> Option<Value> {
    if !slim {
        return Some(pr_next_menu(query, plan.content(), patch_mode, review, raw));
    }
    let first_inventory =
        file_read && patch_mode == "none" && !query.later_page() && !query.has_file_filter();
    if !first_inventory {
        return None;
    }
    let all_patches = raw
        .get("changed_files")
        .and_then(Value::as_u64)
        .is_some_and(|files| files <= INVENTORY_ALL_PATCHES_FILES);
    let mut next = pr_next_menu(query, plan.content(), patch_mode, review, raw);
    if let Some(menu) = next.as_object_mut() {
        menu.retain(|name, _| {
            name == "readSelectedPatches" || (all_patches && name == "readPatches")
        });
    }
    Some(next).filter(|next| next.as_object().is_some_and(|next| !next.is_empty()))
}

/// A shaped pull-request row and what the response still needs from it.
struct ShapedPr {
    row: Value,
    files: FileOutcome,
    /// Surfaces whose minified view dropped text (`next.readRawBody`).
    minified: Vec<&'static str>,
    menu: Option<Value>,
    /// The code the first anchored review comment discusses.
    comment_read: Option<Value>,
}

async fn shape_pr<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &ContentWants,
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
    let patch_mode = wants.patch_mode.as_str();
    let (comments, comments_state, bot_warnings) =
        merge_comments(inline, discussion, wants.include_bots);
    let slim = (query.later_page() || wants.files) && !query.debug();
    let mut row = pr_metadata(&raw, query);
    if slim {
        slim_row(&mut row, query, patch_mode != "none");
    }
    if !bot_warnings.is_empty() {
        row["sanitizationWarnings"] = json!(bot_warnings);
    }
    let mut pagination = Map::new();
    let mut minified = Vec::new();
    if wants.body {
        let raw_body = raw.get("body").and_then(Value::as_str).unwrap_or("");
        let body = history_body_view(raw_body, query);
        if view_dropped_text(raw_body, &body) {
            minified.push("body");
        }
        let (text, page) = paginate_text(&body, query.char_offset(), query.char_length());
        row["body"] = json!(text);
        pagination.insert("body".into(), page);
    }
    let widened;
    let query = match &files {
        Some(loaded) if patch_mode != "none" => {
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
    let mut comment_read = None;
    if let Some(state) = comments_state {
        let shape = shape_pr_comments(&mut row, &mut pagination, comments, state, query);
        if shape.dropped {
            minified.push("comments");
        }
        comment_read = shape.code_read;
    }
    if let Some(loaded) = reviews
        && shape_pr_reviews(&mut row, &mut pagination, loaded.items, loaded.state, query)
    {
        minified.push("reviews");
    }
    if !minified.is_empty() {
        row["bodyView"] = json!("minified");
    }
    if let Some(loaded) = commits {
        shape_pr_commits(
            transport,
            &mut row,
            &mut pagination,
            loaded.items,
            loaded.state,
            query,
            context,
        )
        .await?;
    }
    let menu = read_menu(
        query,
        plan,
        patch_mode,
        &file_outcome.review,
        &raw,
        slim,
        wants.files,
    );
    // Patch rows are the evidence of a patch read: it re-proves only number,
    // state, and the head it read. The metadata read names the PR (title,
    // author, createdAt); a read without patch rows keeps them.
    let patch_rows = row["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty() && files.iter().all(Value::is_object));
    if slim
        && patch_mode != "none"
        && patch_rows
        && row.get("sourceSha").is_some()
        && let Some(fields) = row.as_object_mut()
    {
        for key in ["title", "author", "createdAt"] {
            fields.remove(key);
        }
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    Ok(ShapedPr {
        row,
        files: file_outcome,
        minified,
        menu,
        comment_read,
    })
}

/// A scope or literal that matched nothing: an empty row with a recovery
/// tip, or, when other sections hold evidence, a warning.
fn mark_unmatched(
    out: &mut Value,
    query: &HistoryItemRequest,
    plan: &PrPlan,
    files: &FileOutcome,
    other_evidence: bool,
) -> Option<String> {
    let mut needle_missed = None;
    if files.scope_unmatched && files.unsearched.is_empty() && !files.no_selected_files_matched {
        match query.match_string().filter(|_| plan.needle.is_some()) {
            Some(literal) if plan.scope.is_none() && other_evidence => {
                needle_missed = Some(format!(
                    "matchString {literal:?} hit no patch line in the PR's changed files."
                ));
            }
            Some(_) => {
                out["status"] = json!("empty");
                out["hints"] = json!([
                    "matchString hit no patch line; check its spelling, drop it, or read the inventory (sections:[\"files\"])."
                ]);
            }
            None => {
                out["status"] = json!("empty");
                out["hints"] = json!([
                    "No changed file matched include/status/minChanges; read the inventory (sections:[\"files\"]) and copy a path."
                ]);
            }
        }
    }
    if files.no_selected_files_matched {
        out["status"] = json!("empty");
        out["hints"] = json!([
            "No changed file matched include or patchRanges; copy a path from files or request sections:[\"files\"]."
        ]);
    }
    needle_missed
}

fn finish_pr(
    query: &HistoryItemRequest,
    wants: &ContentWants,
    plan: &PrPlan,
    shaped: ShapedPr,
    graphql_fallback: Option<String>,
) -> Value {
    let ShapedPr {
        row,
        files,
        minified,
        menu,
        comment_read,
    } = shaped;
    let patch_mode = wants.patch_mode.as_str();
    // A `matchString` miss in the files is an empty row only when no other
    // section of the read (body, comments, reviews) holds evidence;
    // otherwise the row says so in a warning.
    let other_evidence = wants.body
        || ["comments", "reviews"].iter().any(|key| {
            row.get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        });
    let mut out = json!({"pullRequests":[row]});
    let needle_missed = mark_unmatched(&mut out, query, plan, &files, other_evidence);
    if !files.unsearched.is_empty() {
        out["pullRequests"][0]["unsearchedFiles"] = json!(files.unsearched);
    }
    if (patch_mode != "none" || plan.needle.is_some())
        && let Some(warning) = clamp_warning(query)
    {
        push_warning(&mut out, warning);
    }
    if let Some(warning) = needle_missed {
        push_warning(&mut out, warning);
    }
    promote_pr_continuations(&mut out, query);
    attach_full_patch_continuation(&mut out, query);
    attach_raw_body_read(&mut out, query, &minified);
    if let Some(path) = &files.first_unsearched {
        attach_unsearched_read(&mut out, query, path);
    }
    attach_context_reads(&mut out, query, &files.clipped);
    // The row's next steps ride the response's leads (one capped list),
    // never a second list nested in the row; the contract's lead priority
    // keeps every lead under the lead cap.
    let merge_read = files.merge_read.filter(|_| patch_mode != "none");
    // A patch read's next step is the code at either side of its numbered
    // diff: those reads rank ahead of the section menu.
    let leads = files
        .side_reads
        .into_iter()
        .map(|(name, read)| (name.to_owned(), read))
        .chain(
            menu.and_then(|menu| match menu {
                Value::Object(menu) => Some(menu),
                _ => None,
            })
            .into_iter()
            .flatten(),
        )
        .chain(merge_read.map(|read| ("readAtMerge".to_owned(), read)));
    for (name, lead) in leads {
        if !out.get("next").is_some_and(Value::is_object) {
            out["next"] = json!({});
        }
        if let Some(next) = out["next"].as_object_mut() {
            next.entry(name).or_insert(lead);
        }
    }
    // The comments' own lead goes before the menu's: it is the read the
    // comment page asked for.
    if let Some(read) = comment_read {
        if !out.get("next").is_some_and(Value::is_object) {
            out["next"] = json!({});
        }
        if let Some(next) = out["next"].as_object_mut() {
            next.shift_insert(0, "readCommentCode".to_owned(), read);
        }
    }
    if !query.debug() {
        trim_content_pagination(&mut out);
    } else if let Some(reason) = graphql_fallback {
        out["graphqlFallback"] = json!(reason);
    }
    out
}

/// A `matchString` search that skipped patchless files covers only part of
/// the diff: mark it partial and offer the literal search of the first
/// skipped file's source at the PR head (a template for the rest).
fn attach_unsearched_read(out: &mut Value, query: &HistoryItemRequest, path: &str) {
    let source_sha = str_at(out, "/pullRequests/0/sourceSha").map(str::to_owned);
    out["isPartial"] = json!(true);
    match out.get_mut("partialReasons").and_then(Value::as_array_mut) {
        Some(reasons) => reasons.push(json!("patchUnavailable")),
        None => out["partialReasons"] = json!(["patchUnavailable"]),
    }
    let (Some(needle), Some(sha)) = (query.match_string(), source_sha) else {
        return;
    };
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"]["searchUnpatchedFile"] = crate::tools::result::Continuation::new(
        ToolId::GhGetFileContent,
        json!({
            "owner": query.owner(),
            "repo": query.repo(),
            "path": path,
            "ref": sha,
            "matchString": needle,
        }),
    )
    .confidence("high")
    .build();
}

/// `matchString` context the hunks cut short and the head text did not
/// fill: the row is partial, and `next.expandContext` reads each file's
/// wanted windows at the PR head (`sourceSha`).
fn attach_context_reads(
    out: &mut Value,
    query: &HistoryItemRequest,
    clipped: &[(String, Vec<String>)],
) {
    let Some(sha) = str_at(out, "/pullRequests/0/sourceSha").map(str::to_owned) else {
        return;
    };
    if clipped.is_empty() {
        return;
    }
    out["isPartial"] = json!(true);
    match out.get_mut("partialReasons").and_then(Value::as_array_mut) {
        Some(reasons) => reasons.push(json!("contextClipped")),
        None => out["partialReasons"] = json!(["contextClipped"]),
    }
    push_warning(
        out,
        format!(
            "contextLines {} reaches past the diff hunks of {} file(s) (contextClipped); next.expandContext reads those lines at sourceSha.",
            match_context(query),
            clipped.len()
        ),
    );
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    for (index, (path, ranges)) in clipped.iter().enumerate() {
        let name = if index == 0 {
            "expandContext".to_owned()
        } else {
            format!("expandContext{}", index + 1)
        };
        out["next"][name] = crate::tools::result::Continuation::new(
            ToolId::GhGetFileContent,
            json!({
                "owner": query.owner(),
                "repo": query.repo(),
                "path": path,
                "ref": sha,
                "ranges": ranges,
            }),
        )
        .confidence("exact")
        .build();
    }
}

/// Default responses drop pagination that adds nothing once `next.*` is
/// built: a finished single-page list or whole text window, and the patch
/// cursor, which the cut file's own `patchPagination` already carries.
/// Provider limits stay.
fn trim_content_pagination(out: &mut Value) {
    let Some(row) = out
        .pointer_mut("/pullRequests/0")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let Some(pages) = row
        .get_mut("contentPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    pages.retain(|_, page| {
        let done =
            page.get("hasMore") == Some(&Value::Bool(false)) && page.get("terminalLimit").is_none();
        // A finished first page, or a whole text window (offset zero).
        let first = page.get("currentPage").and_then(Value::as_u64) == Some(1)
            || page.get("offset").and_then(Value::as_u64) == Some(0);
        !(done && first)
    });
    if pages
        .get("patches")
        .is_some_and(|patches| patches.get("hasMore") != Some(&Value::Bool(true)))
    {
        pages.remove("patches");
    }
    if let Some(patches) = pages.get_mut("patches").and_then(Value::as_object_mut) {
        patches.retain(|key, _| matches!(key.as_str(), "hasMore" | "unfinishedFiles"));
    }
    if pages.is_empty() {
        row.remove("contentPagination");
    }
}

/// Characters a PR summary's `bodyPreview` shows.
const BODY_PREVIEW_CHARS: usize = 300;

/// The opening of a PR body's (template-minified) view, whitespace runs
/// collapsed; a cut preview ends with `…`. `None` for an empty body.
fn body_preview(body: &str, query: &HistoryItemRequest) -> Option<String> {
    let view = history_body_view(body, query);
    let flat = view.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    if flat.chars().count() <= BODY_PREVIEW_CHARS {
        return Some(flat);
    }
    let mut preview = flat.chars().take(BODY_PREVIEW_CHARS).collect::<String>();
    preview.push('…');
    Some(preview)
}

fn pr_metadata(raw: &Value, query: &HistoryItemRequest) -> Value {
    let merged = raw.get("merged_at").is_some_and(|v| !v.is_null());
    let open = !merged && str_at(raw, "/state").unwrap_or("open") == "open";
    let labels = raw
        .get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| str_at(v, "/name").map(str::to_owned))
        })
        .collect::<Vec<_>>();
    // The body is opt-in (`include:["body"]`, offered by the menu): the
    // summary names the change; a merged PR's close time is its merge time,
    // and only an open PR's last update says whether it is still moving.
    let mut row = json!({
        "number": raw["number"],
        "title": string(raw.get("title")),
        "state": if merged {"merged"} else {str_at(raw,"/state").unwrap_or("open")},
        "draft": raw.get("draft").and_then(Value::as_bool).filter(|v|*v),
        "author": str_at(raw,"/user/login").unwrap_or(""),
        "labels": (!labels.is_empty()).then_some(labels),
        // GraphQL reads the first 20 labels; say when more exist.
        "labelsTruncated": raw.get("labels_truncated").and_then(Value::as_bool).filter(|v| *v),
        "targetBranch": str_at(raw,"/base/ref").filter(|v|!v.is_empty()),
        "sourceBranch": str_at(raw,"/head/ref").filter(|v|!v.is_empty()),
        "sourceSha": str_at(raw,"/head/sha").filter(|v|!v.is_empty()),
        // The target-branch tip GitHub recorded for the PR: not the diff's
        // old side (that is the merge base, which `readParent` reads).
        "targetSha": str_at(raw,"/base/sha").filter(|v|!v.is_empty()),
        "createdAt": string(raw.get("created_at")),
        "updatedAt": open.then(|| string(raw.get("updated_at"))).filter(|v| !v.is_empty()),
        "closedAt": (!merged).then(|| raw.get("closed_at").filter(|v|!v.is_null())).flatten(),
        "mergedAt": raw.get("merged_at").filter(|v|!v.is_null()),
        // Only a merged PR has a real merge commit (open PRs expose GitHub's
        // test-merge SHA, which is not on the base branch).
        "mergeCommitSha": merged.then(|| str_at(raw,"/merge_commit_sha")).flatten().filter(|v|!v.is_empty()),
        "commentsCount": nonzero(raw.get("comments")),
        "commitsCount": nonzero(raw.get("commits")),
        // Review threads (GraphQL only): REST counts review comments, a
        // different unit, so a REST read leaves this out.
        "reviewThreadsCount": nonzero(raw.get("review_threads")),
        "changedFilesCount": nonzero(raw.get("changed_files")),
        "additions": nonzero(raw.get("additions")),
        "deletions": nonzero(raw.get("deletions")),
    });
    if query.content_value().is_none()
        && raw.get("draft") == Some(&Value::Bool(false))
        && let Some(row) = row.as_object_mut()
    {
        row.remove("draft");
    }
    // A summary previews what the PR says; the menu's body read (`readFiles`,
    // `readPatches` or `readBody`) holds the whole body.
    if query.content_value().is_none()
        && let Some(preview) = str_at(raw, "/body").and_then(|body| body_preview(body, query))
    {
        row["bodyPreview"] = json!(preview);
    }
    if query.debug() {
        // Diagnostics keep the provider's timestamps whole.
        if let Some(updated) = raw.get("updated_at").filter(|v| v.is_string()) {
            row["updatedAt"] = updated.clone();
        }
        if let Some(closed) = raw.get("closed_at").filter(|v| !v.is_null()) {
            row["closedAt"] = closed.clone();
        }
    }
    remove_nulls(&mut row);
    row
}

#[cfg(test)]
mod tests {
    use super::super::util::merge;
    use super::*;

    fn summary(raw: Value, debug: bool) -> Value {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "number":1,"debug":debug
        }))
        .expect("query");
        pr_metadata(&raw, &query)
    }

    #[test]
    fn summary_rows_state_each_time_once_and_preview_the_body() {
        let base = json!({"number":1,"title":"t","user":{"login":"a"},"body":"Long description",
            "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-05T00:00:00Z"});
        let merged = summary(
            merge(
                base.clone(),
                json!({"state":"closed","merged_at":"2026-01-04T00:00:00Z",
                "closed_at":"2026-01-04T00:00:01Z","merge_commit_sha":"abc"}),
            ),
            false,
        );
        assert_eq!(merged["mergedAt"], "2026-01-04T00:00:00Z", "{merged}");
        for absent in ["closedAt", "updatedAt", "body"] {
            assert!(merged.get(absent).is_none(), "{absent}: {merged}");
        }
        // The summary previews the body; the menu's body read holds it whole.
        assert_eq!(merged["bodyPreview"], "Long description", "{merged}");
        let closed = summary(
            merge(
                base.clone(),
                json!({"state":"closed","merged_at":null,
                "closed_at":"2026-01-04T00:00:00Z"}),
            ),
            false,
        );
        assert_eq!(closed["closedAt"], "2026-01-04T00:00:00Z", "{closed}");
        assert!(closed.get("updatedAt").is_none(), "{closed}");
        let open = summary(merge(base.clone(), json!({"state":"open"})), false);
        assert_eq!(open["updatedAt"], "2026-01-05T00:00:00Z", "{open}");
        // debug keeps the provider's timestamps whole.
        let debug = summary(
            merge(
                base,
                json!({"state":"closed","merged_at":"2026-01-04T00:00:00Z",
                "closed_at":"2026-01-04T00:00:01Z"}),
            ),
            true,
        );
        assert_eq!(debug["closedAt"], "2026-01-04T00:00:01Z", "{debug}");
        assert_eq!(debug["updatedAt"], "2026-01-05T00:00:00Z", "{debug}");
    }

    /// HI4: every PR view names the change's three commits: the head
    /// (`sourceSha`), the recorded target-branch tip (`targetSha`) and a
    /// merged PR's merge commit, patch and later pages included.
    #[test]
    fn every_pr_view_keeps_source_target_and_merge_commits() {
        let raw = json!({"number":1,"title":"t","user":{"login":"a"},"state":"closed",
            "merged_at":"2026-01-04T00:00:00Z","merge_commit_sha":"m1",
            "head":{"sha":"h1","ref":"feat"},"base":{"sha":"b1","ref":"main"},
            "created_at":"2026-01-01T00:00:00Z","changed_files":3,"additions":5,"deletions":1,
            "commits":4,"review_threads":2});
        let row = summary(raw.clone(), false);
        assert_eq!(
            (&row["sourceSha"], &row["targetSha"], &row["mergeCommitSha"]),
            (&json!("h1"), &json!("b1"), &json!("m1")),
            "{row}"
        );
        assert_eq!(row["commitsCount"], 4, "{row}");
        assert_eq!(row["reviewThreadsCount"], 2, "{row}");
        for fields in [
            json!({"sections":["patches"]}),
            json!({"sections":["files"]}),
            json!({"sections":["patches"],"matchString":"x"}),
            json!({"sections":["patches"],"filePage":2}),
        ] {
            let query = HistoryItemRequest::from_row(merge(
                json!({"operation":"pullRequest","owner":"o","repo":"r","number":1}),
                fields.clone(),
            ))
            .expect("query");
            let mut row = pr_metadata(&raw, &query);
            slim_row(&mut row, &query, fields["sections"] == json!(["patches"]));
            for key in ["sourceSha", "targetSha", "mergeCommitSha"] {
                assert!(row.get(key).is_some(), "{key} {fields}: {row}");
            }
        }
        // An open PR has no merge commit; REST has no review-thread total.
        let open = summary(
            json!({"number":1,"state":"open","head":{"sha":"h1"},"base":{"sha":"b1"},
                "merge_commit_sha":"test-merge","review_comments":9}),
            false,
        );
        assert!(open.get("mergeCommitSha").is_none(), "{open}");
        assert!(open.get("reviewThreadsCount").is_none(), "{open}");
        assert_eq!(open["targetSha"], "b1", "{open}");
    }
}
