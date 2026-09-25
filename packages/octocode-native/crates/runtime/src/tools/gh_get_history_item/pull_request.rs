//! `operation: "pullRequest"`: concurrent collection loads (GraphQL first page
//! or REST windows), metadata row, and assembly of the shaped sections.
use super::continuations::{pr_next_menu, promote_pr_continuations};
use super::files::{FileFilter, patch_selection, shape_pr_files};
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
    WindowState, load_window,
};
use super::{DEFAULT_PAGE_SIZE, GhGetHistoryItemQuery, fetch, validation};
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

pub(super) fn content_wants(query: &GhGetHistoryItemQuery) -> ContentWants {
    let content = query.content.as_ref().and_then(Value::as_object);
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
    query: &GhGetHistoryItemQuery,
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
        .number
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let content = query.content.as_ref().and_then(Value::as_object);
    let patch_selector = content
        .and_then(|c| c.get("patches"))
        .and_then(Value::as_object);
    let patch_mode = wants.patch_mode.as_str();
    let include_bots = wants.include_bots;
    let page_size = query.page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, 100);
    let selection = patch_selection(patch_selector);
    let needle = needle(query);
    let file_filter = FileFilter {
        selected: &selection.0,
        needle: needle.as_deref(),
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
    };
    let pulls = ["repos", &query.owner, &query.repo, "pulls", &number];
    let issues = ["repos", &query.owner, &query.repo, "issues", &number];
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
    let raw_load = async {
        match graphql.as_ref() {
            Some(graphql) => Ok(graphql.raw.clone()),
            None => fetch(transport, &pulls, &[], context)
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
            spec(MAX_FILE_BATCHES, query.file_page, !file_filter.is_trivial()),
            |value| file_filter.matches(value),
            context,
        ),
        load_collection(
            transport,
            wants.discussion,
            complete(|g| g.discussion, map_graphql_comments),
            &discussion_path,
            spec(MAX_COLLECTION_BATCHES, query.comment_page, true),
            comment_filter,
            context,
        ),
        load_collection(
            transport,
            wants.inline,
            None,
            &inline_path,
            spec(MAX_COLLECTION_BATCHES, query.comment_page, true),
            comment_filter,
            context,
        ),
        load_collection(
            transport,
            wants.reviews,
            complete(|g| g.reviews, map_graphql_reviews),
            &reviews_path,
            spec(MAX_COLLECTION_BATCHES, query.review_page, needle.is_some()),
            body_filter,
            context,
        ),
        load_collection(
            transport,
            wants.commits,
            complete(|g| g.commits, map_graphql_commits),
            &commits_path,
            spec(MAX_PR_COMMIT_BATCHES, query.commit_page, false),
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

    let mut row = pr_metadata(&raw, query, wants.body);
    if !sanitization_warnings.is_empty() {
        row["sanitizationWarnings"] = json!(sanitization_warnings);
    }
    let mut content_pagination = Map::new();
    if wants.body {
        let body = history_body_view(raw.get("body").and_then(Value::as_str).unwrap_or(""), query);
        let (text, pagination) = paginate_text(&body, query.char_offset, query.char_length);
        row["body"] = json!(text);
        content_pagination.insert("body".into(), pagination);
    }
    let mut no_selected_files_matched = false;
    if let Some(loaded) = files_loaded {
        no_selected_files_matched = shape_pr_files(
            &mut row,
            &mut content_pagination,
            loaded.items,
            loaded.state,
            query,
            patch_selector,
            patch_mode,
        );
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
    let first_changed_path = row
        .get("changedFiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|v| str_at(v, "/path"));
    row["next"] = pr_next_menu(query, content, patch_mode, first_changed_path);
    if !content_pagination.is_empty() {
        row["contentPagination"] = Value::Object(content_pagination);
    }
    let mut out = json!({"type":"pullRequests","pullRequests":[row]});
    if no_selected_files_matched {
        out["status"] = json!("empty");
        out["errorCode"] = json!("noSelectedFilesMatched");
        out["hints"] = json!([
            "No changed file matched the requested patches.files or patches.ranges path. Copy a path from content.changedFiles or request changedFiles:true first."
        ]);
    }
    promote_pr_continuations(&mut out, query);
    Ok(out)
}

fn pr_metadata(raw: &Value, query: &GhGetHistoryItemQuery, body_requested: bool) -> Value {
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
        "commentsCount": nonzero(raw.get("comments")),
        "changedFilesCount": nonzero(raw.get("changed_files")),
        "additions": nonzero(raw.get("additions")),
        "deletions": nonzero(raw.get("deletions")),
        "bodyPreview": (!body_requested && !body.is_empty()).then(|| compact(body,500)),
    });
    if query.content.is_none()
        && raw.get("draft") == Some(&Value::Bool(false))
        && let Some(row) = row.as_object_mut()
    {
        row.remove("draft");
    }
    remove_nulls(&mut row);
    row
}
