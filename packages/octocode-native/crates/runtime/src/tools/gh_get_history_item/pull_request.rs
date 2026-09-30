//! `operation: "pullRequest"`: concurrent collection loads (GraphQL first page
//! or REST windows), metadata row, and assembly of the shaped sections.
use super::continuations::{
    BODY_PREVIEW_CHARS, INVENTORY_ALL_PATCHES_FILES, attach_full_patch_continuation, pr_next_menu,
    promote_pr_continuations,
};
use super::files::{FileFilter, InventoryFilter, file_page_size, patch_selection, shape_pr_files};
use super::graphql::{
    GraphqlCollection, GraphqlPr, graphql_complete_collection_eligible, graphql_pull_request,
    map_graphql_comments, map_graphql_commits, map_graphql_files, map_graphql_reviews,
};
use super::pr_sections::{shape_pr_comments, shape_pr_commits, shape_pr_reviews};
use super::util::{
    body_matches, compact, content_flag, history_body_view, is_bot, map_comments, needle, nonzero,
    paginate_text, str_at, string,
};
use super::window::{
    Loaded, MAX_COLLECTION_BATCHES, MAX_FILE_BATCHES, MAX_PR_COMMIT_BATCHES, WindowSpec,
    WindowState, load_window, reconcile_file_totals,
};
use super::{HistoryItemRequest, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, RequestContext,
};
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
        files: content_flag(content, "changedFiles") || patch_mode != "none",
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
    let graphql = if graphql_complete_collection_eligible(query) {
        graphql_pull_request(transport, query, context, &wants)
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let number = query
        .number()
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let content_value = query.content_value();
    let content = content_value.as_ref().and_then(Value::as_object);
    let patch_selector = content
        .and_then(|c| c.get("patches"))
        .and_then(Value::as_object);
    let patch_mode = wants.patch_mode.as_str();
    let include_bots = wants.include_bots;
    let page_size = query.collection_page_size();
    let selection = patch_selection(patch_selector);
    let needle = needle(query);
    let scope = InventoryFilter::from_query(query).map_err(|message| validation(&message))?;
    let file_filter = FileFilter {
        selected: &selection.0,
        needle: needle.as_deref(),
        scope: scope.as_ref(),
    };
    let body_filter = |value: &Value| body_matches(value, needle.as_deref());
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
    let pulls = ["repos", query.owner(), query.repo(), "pulls", &number];
    let issues = ["repos", query.owner(), query.repo(), "issues", &number];
    let files_path = [pulls.as_slice(), &["files"]].concat();
    let discussion_path = [issues.as_slice(), &["comments"]].concat();
    let inline_path = [pulls.as_slice(), &["comments"]].concat();
    let reviews_path = [pulls.as_slice(), &["reviews"]].concat();
    let commits_path = [pulls.as_slice(), &["commits"]].concat();
    let complete = |kind: fn(&GraphqlPr) -> GraphqlCollection, map: fn(&Value) -> Vec<Value>| {
        graphql
            .as_ref()
            .filter(|g| kind(g) == GraphqlCollection::Complete)
            .map(|g| map(&g.source))
    };
    // A selected or match-filtered file scan cannot jump to a batch by index;
    // with the PR's changed-file count in hand it reads every batch at once.
    let filtered_files = wants.files
        && !file_filter.is_trivial()
        && complete(|g| g.files, map_graphql_files).is_none();
    let raw_first = match graphql.as_ref() {
        None if filtered_files => Some(fetch(transport, &pulls, &[], context).await?.0),
        _ => None,
    };
    let file_total = raw_first
        .as_ref()
        .and_then(|raw| raw.get("changed_files"))
        .and_then(Value::as_u64)
        .map(|total| usize::try_from(total).unwrap_or(usize::MAX));
    let raw_load = async {
        match (graphql.as_ref(), raw_first.clone()) {
            (Some(graphql), _) => Ok(graphql.raw.clone()),
            (None, Some(raw)) => Ok(raw),
            (None, None) => fetch(transport, &pulls, &[], context)
                .await
                .map(|(raw, _)| raw),
        }
    };
    // Independent REST collections load concurrently; each one derives its
    // provider batches from the public page cursor (no provider cursors leak
    // into continuations).
    let (raw, files_loaded, discussion_loaded, inline_loaded, reviews_loaded, commits_loaded) = tokio::try_join!(
        raw_load,
        load_collection(
            transport,
            wants.files,
            complete(|g| g.files, map_graphql_files),
            &files_path,
            WindowSpec {
                provider_total: file_total,
                page_size: file_page_size(query, patch_mode != "none"),
                ..spec(
                    MAX_FILE_BATCHES,
                    query.file_page(),
                    !file_filter.is_trivial(),
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
            spec(MAX_COLLECTION_BATCHES, query.comment_page(), true),
            comment_filter,
            context,
        ),
        load_collection(
            transport,
            wants.inline,
            None,
            &inline_path,
            spec(MAX_COLLECTION_BATCHES, query.comment_page(), true),
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

    // Inline review comments sort before discussion; the combined list is
    // complete only when both sources are.
    let mut comments = Vec::new();
    let mut comments_state: Option<WindowState> = None;
    let mut sanitization_warnings = Vec::new();
    for (loaded, kind, label) in [
        (inline_loaded, "review_inline", "inline "),
        (discussion_loaded, "discussion", ""),
    ] {
        let Some(loaded) = loaded else { continue };
        comments_state = Some(match comments_state {
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
            sanitization_warnings.push(format!(
                "{dropped} bot {label}comment(s) hidden (set content.comments.includeBots:true to include)"
            ));
        }
    }

    // A later page already holds the header and the menu from page one,
    // and a file inventory or patch read is read for its files: both carry
    // only the identity fields every page must re-prove, plus the fields the
    // output contract requires of every pull-request row.
    let file_read = wants.files;
    let slim = (query.later_page() || file_read) && !query.debug();
    let mut row = pr_metadata(&raw, query, wants.body);
    if slim && let Some(fields) = row.as_object_mut() {
        // A patch read re-proves only the head it read; a list page also
        // keeps the merge commit and file count, and a first inventory page
        // the diff size it lists.
        let patches = patch_mode != "none";
        let totals = !query.later_page() && !patches;
        fields.retain(|key, _| {
            matches!(
                key.as_str(),
                "number" | "title" | "state" | "author" | "createdAt" | "sourceSha"
            ) || (!patches && matches!(key.as_str(), "mergeCommitSha" | "changedFilesCount"))
                || (totals && matches!(key.as_str(), "additions" | "deletions"))
        });
    }
    if !sanitization_warnings.is_empty() {
        row["sanitizationWarnings"] = json!(sanitization_warnings);
    }
    let mut content_pagination = Map::new();
    if wants.body {
        let body = history_body_view(raw.get("body").and_then(Value::as_str).unwrap_or(""), query);
        let (text, pagination) = paginate_text(&body, query.char_offset(), query.char_length());
        row["body"] = json!(text);
        content_pagination.insert("body".into(), pagination);
    }
    let mut no_selected_files_matched = false;
    let mut first_changed_path = None;
    let mut unsearched = Vec::new();
    let mut first_unsearched = None;
    if let Some(loaded) = files_loaded {
        let listed = loaded.state.skipped + loaded.items.len();
        let state = loaded.state;
        let shaped = shape_pr_files(
            &mut row,
            &mut content_pagination,
            loaded.items,
            state,
            query,
            patch_selector,
            patch_mode,
            scope.as_ref(),
        );
        no_selected_files_matched = shaped.no_selected_match;
        first_changed_path = shaped.patch_target;
        unsearched = shaped.unsearched;
        first_unsearched = shaped.first_unsearched;
        if let Some(page) = content_pagination.get_mut("changedFiles") {
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
    }
    if let Some(state) = comments_state {
        shape_pr_comments(&mut row, &mut content_pagination, comments, state, query);
    }
    if let Some(loaded) = reviews_loaded {
        shape_pr_reviews(
            &mut row,
            &mut content_pagination,
            loaded.items,
            loaded.state,
            query,
        );
    }
    if let Some(loaded) = commits_loaded {
        shape_pr_commits(
            transport,
            &mut row,
            &mut content_pagination,
            loaded.items,
            loaded.state,
            query,
            context,
        )
        .await?;
    }
    if !slim {
        row["next"] = pr_next_menu(
            query,
            content,
            patch_mode,
            first_changed_path.as_deref(),
            &raw,
        );
    } else if file_read
        && patch_mode == "none"
        && !query.later_page()
        && query.file_filter().is_none()
    {
        // An unfiltered inventory's own next steps: the ranked selected
        // patch (and every patch on a small PR) and the merge commit. A
        // filtered inventory or a patch read is a targeted answer and keeps
        // only its continuations.
        let all_patches = raw
            .get("changed_files")
            .and_then(Value::as_u64)
            .is_some_and(|files| files <= INVENTORY_ALL_PATCHES_FILES);
        let mut menu = pr_next_menu(
            query,
            content,
            patch_mode,
            first_changed_path.as_deref(),
            &raw,
        );
        if let Some(menu) = menu.as_object_mut() {
            menu.retain(|name, _| {
                matches!(name.as_str(), "getSelectedPatches" | "getMergeCommit")
                    || (all_patches && name == "getAllPatches")
            });
        }
        if menu.as_object().is_some_and(|menu| !menu.is_empty()) {
            row["next"] = menu;
        }
    }
    // Patch rows are the evidence of a patch read: it re-proves only number,
    // state, and the head it read. The metadata read names the PR (title,
    // author, createdAt); a read without patch rows keeps them.
    let patch_rows = row["changedFiles"]
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
    if !content_pagination.is_empty() {
        row["contentPagination"] = Value::Object(content_pagination);
    }
    let mut out = json!({"type":"pullRequests","pullRequests":[row]});
    if no_selected_files_matched {
        out["status"] = json!("empty");
        out["errorCode"] = json!("noSelectedFilesMatched");
        out["hints"] = json!([
            "No changed file matched patches.files or patches.ranges; copy a path from changedFiles or request changedFiles:true."
        ]);
    }
    if !unsearched.is_empty() {
        out["pullRequests"][0]["unsearchedFiles"] = json!(unsearched);
    }
    promote_pr_continuations(&mut out, query);
    attach_full_patch_continuation(&mut out, query);
    if let Some(path) = first_unsearched {
        attach_unsearched_read(&mut out, query, &path);
    }
    if !query.debug() {
        trim_content_pagination(&mut out);
    }
    Ok(out)
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
    out["next"]["searchUnpatchedFile"] = json!({
        "tool": "ghGetFileContent",
        "confidence": "high",
        "query": {
            "owner": query.owner(),
            "repo": query.repo(),
            "path": path,
            "branch": sha,
            "matchString": needle,
        },
    });
}

/// Default responses drop pagination that adds nothing once `next.*` is
/// built: a finished single-page list and the patch cursor, which the cut
/// file's own `patchPagination` already carries. Provider limits stay.
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
        let finished_single = page.get("hasMore") == Some(&Value::Bool(false))
            && page.get("currentPage").and_then(Value::as_u64) == Some(1)
            && page.get("terminalLimit").is_none();
        !finished_single
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

fn pr_metadata(raw: &Value, query: &HistoryItemRequest, body_requested: bool) -> Value {
    let merged = raw.get("merged_at").is_some_and(|v| !v.is_null());
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
    let body = raw.get("body").and_then(Value::as_str).unwrap_or("");
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
        "createdAt": string(raw.get("created_at")),
        "updatedAt": string(raw.get("updated_at")),
        "closedAt": raw.get("closed_at").filter(|v|!v.is_null()),
        "mergedAt": raw.get("merged_at").filter(|v|!v.is_null()),
        // Only a merged PR has a real merge commit (open PRs expose GitHub's
        // test-merge SHA, which is not on the base branch).
        "mergeCommitSha": merged.then(|| str_at(raw,"/merge_commit_sha")).flatten().filter(|v|!v.is_empty()),
        "commentsCount": nonzero(raw.get("comments")),
        "changedFilesCount": nonzero(raw.get("changed_files")),
        "additions": nonzero(raw.get("additions")),
        "deletions": nonzero(raw.get("deletions")),
        "bodyPreview": (!body_requested && !body.is_empty()).then(|| compact(body,BODY_PREVIEW_CHARS)),
    });
    if query.content_value().is_none()
        && raw.get("draft") == Some(&Value::Bool(false))
        && let Some(row) = row.as_object_mut()
    {
        row.remove("draft");
    }
    remove_nulls(&mut row);
    row
}
