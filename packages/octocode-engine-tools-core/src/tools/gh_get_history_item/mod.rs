use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
};
use crate::tools::local_fetch::ContentScan;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{collections::HashMap, path::Path};

const DEFAULT_PAGE_SIZE: usize = 30;
const DEFAULT_TEXT_WINDOW: usize = 12_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GhGetHistoryItemQuery {
    pub operation: ItemOperation,
    pub owner: String,
    pub repo: String,
    pub number: Option<u64>,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub content: Option<Value>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub file_page: Option<usize>,
    pub file_batch: Option<usize>,
    pub comment_page: Option<usize>,
    pub commit_page: Option<usize>,
    pub review_page: Option<usize>,
    pub collection_pages: Option<Value>,
    pub include_diff: Option<bool>,
    pub path: Option<String>,
    pub char_offset: Option<usize>,
    pub char_length: Option<usize>,
    pub match_string: Option<String>,
    pub comment_body_offset: Option<usize>,
    pub minify: Option<String>,
    pub goal: Option<String>,
    pub reasoning: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemOperation {
    PullRequest,
    Issue,
    Commit,
    Compare,
}

pub async fn execute<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, ProviderError> {
    let result = execute_inner(transport, query, context).await;
    let mut value = match result {
        Ok(value) => value,
        Err(mut error) => {
            match error.kind {
                ProviderErrorKind::NotFound => {
                    let canonical = "Repository, resource, or path not found";
                    error.message = if matches!(query.operation, ItemOperation::PullRequest) {
                        format!(
                            "Failed to fetch pull request #{}: {canonical}",
                            query.number.unwrap_or_default()
                        )
                        .into_boxed_str()
                    } else {
                        canonical.into()
                    };
                }
                ProviderErrorKind::Permission => {
                    error.message = "Access forbidden - insufficient permissions".into();
                }
                ProviderErrorKind::RateLimited => {
                    if let Some(rate_limit) = error.rate_limit.as_mut()
                        && rate_limit.remaining.is_none()
                    {
                        rate_limit.remaining = Some(0);
                    }
                }
                _ => {}
            }
            error.message = sanitize_text(error.message.as_ref(), security)?.into_boxed_str();
            return Err(error);
        }
    };
    sanitize_all_strings(&mut value, security)?;
    remove_nulls(&mut value);
    enforce_response_limit(&value, context.max_body_bytes)?;
    Ok(value)
}

fn enforce_response_limit(value: &Value, max_body_bytes: usize) -> Result<(), ProviderError> {
    let size = serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if size > max_body_bytes {
        return Err(ProviderError::new(
            ProviderErrorKind::ResponseTooLarge,
            format!("GitHub history item response exceeds {max_body_bytes} bytes"),
        ));
    }
    Ok(())
}

async fn execute_inner<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    validate(query)?;
    check_context(context)?;
    match query.operation {
        ItemOperation::PullRequest => pull_request(transport, query, context).await,
        ItemOperation::Issue => issue(transport, query, context).await,
        ItemOperation::Commit => commit(transport, query, context).await,
        ItemOperation::Compare => compare(transport, query, context).await,
    }
}

fn validate(query: &GhGetHistoryItemQuery) -> Result<(), ProviderError> {
    if query.owner.is_empty() || query.repo.is_empty() {
        return Err(validation("owner and repo are required"));
    }
    match query.operation {
        ItemOperation::PullRequest | ItemOperation::Issue if query.number.is_none() => {
            Err(validation("number is required"))
        }
        ItemOperation::Commit if query.reference.as_deref().is_none_or(str::is_empty) => {
            Err(validation("ref is required"))
        }
        ItemOperation::Compare
            if query.base.as_deref().is_none_or(str::is_empty)
                || query.head.as_deref().is_none_or(str::is_empty) =>
        {
            Err(validation("base and head are required"))
        }
        _ => Ok(()),
    }
}

fn validation(message: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}

fn check_context(context: &RequestContext) -> Result<(), ProviderError> {
    if context.cancellation.is_cancelled() {
        Err(ProviderError::new(
            ProviderErrorKind::Cancelled,
            "request cancelled",
        ))
    } else if std::time::Instant::now() >= context.deadline {
        Err(ProviderError::new(
            ProviderErrorKind::Timeout,
            "request timed out",
        ))
    } else {
        Ok(())
    }
}

async fn fetch<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    segments: &[&str],
    query: &[(&str, String)],
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    check_context(context)?;
    let response = transport.history_item(segments, query, context).await?;
    Ok((response.value, response.has_more))
}

async fn pull_request<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let number = query
        .number
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let (raw, _) = fetch(
        transport,
        &["repos", &query.owner, &query.repo, "pulls", &number],
        &[],
        context,
    )
    .await?;
    let content = query.content.as_ref().and_then(Value::as_object);
    let want_body = content_flag(content, "body");
    let patch_selector = content
        .and_then(|c| c.get("patches"))
        .and_then(Value::as_object);
    let patch_mode = patch_selector
        .and_then(|p| p.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    let want_files = content_flag(content, "changedFiles") || patch_mode != "none";
    let comments_selector = content
        .and_then(|c| c.get("comments"))
        .and_then(Value::as_object);
    let want_discussion = content_flag(comments_selector, "discussion");
    let want_inline = content_flag(comments_selector, "reviewInline");
    let include_bots = content_flag(comments_selector, "includeBots");
    let want_reviews = content_flag(content, "reviews");
    let commits_selector = content
        .and_then(|c| c.get("commits"))
        .and_then(Value::as_object);
    let want_commits = commits_selector.is_some();
    let mut states = Map::new();
    let mut files = Vec::new();
    let mut comments = Vec::new();
    let mut sanitization_warnings = Vec::new();
    let mut reviews = Vec::new();
    let mut commits = Vec::new();

    if want_files {
        let provider_page = collection_page(query, "changedFiles", 1);
        let (value, more) = fetch_collection(
            transport,
            &[
                "repos",
                &query.owner,
                &query.repo,
                "pulls",
                &number,
                "files",
            ],
            provider_page,
            100,
            context,
        )
        .await?;
        files = array(value);
        states.insert(
            "changedFiles".into(),
            json!({"page":provider_page,"hasMore":more}),
        );
    }
    if want_discussion {
        let provider_page = collection_page(query, "discussion", 1);
        let (value, more) = fetch_collection(
            transport,
            &[
                "repos",
                &query.owner,
                &query.repo,
                "issues",
                &number,
                "comments",
            ],
            provider_page,
            100,
            context,
        )
        .await?;
        let values = array(value);
        let dropped = values
            .iter()
            .filter(|v| is_bot(str_at(v, "/user/login").unwrap_or("")))
            .count();
        comments.extend(map_comments(values, "discussion", include_bots));
        if !include_bots && dropped > 0 {
            sanitization_warnings.push(format!(
                "{dropped} bot comment(s) hidden (set content.comments.includeBots:true to include)"
            ));
        }
        states.insert(
            "discussion".into(),
            json!({"page":provider_page,"hasMore":more}),
        );
    }
    if want_inline {
        let provider_page = collection_page(query, "inline", 1);
        let (value, more) = fetch_collection(
            transport,
            &[
                "repos",
                &query.owner,
                &query.repo,
                "pulls",
                &number,
                "comments",
            ],
            provider_page,
            100,
            context,
        )
        .await?;
        let values = array(value);
        let dropped = values
            .iter()
            .filter(|v| is_bot(str_at(v, "/user/login").unwrap_or("")))
            .count();
        comments.extend(map_comments(values, "review_inline", include_bots));
        if !include_bots && dropped > 0 {
            sanitization_warnings.push(format!(
                "{dropped} bot inline comment(s) hidden (set content.comments.includeBots:true to include)"
            ));
        }
        states.insert(
            "inline".into(),
            json!({"page":provider_page,"hasMore":more}),
        );
    }
    if want_reviews {
        let provider_page = collection_page(query, "reviews", 1);
        let (value, more) = fetch_collection(
            transport,
            &[
                "repos",
                &query.owner,
                &query.repo,
                "pulls",
                &number,
                "reviews",
            ],
            provider_page,
            100,
            context,
        )
        .await?;
        reviews = array(value);
        states.insert(
            "reviews".into(),
            json!({"page":provider_page,"hasMore":more}),
        );
    }
    if want_commits {
        let provider_page = collection_page(query, "commits", 1);
        let (value, more) = fetch_collection(
            transport,
            &[
                "repos",
                &query.owner,
                &query.repo,
                "pulls",
                &number,
                "commits",
            ],
            provider_page,
            50,
            context,
        )
        .await?;
        commits = array(value);
        states.insert(
            "commits".into(),
            json!({"page":provider_page,"hasMore":more}),
        );
    }

    let mut row = pr_metadata(&raw, query, want_body);
    if !sanitization_warnings.is_empty() {
        row["sanitizationWarnings"] = json!(sanitization_warnings);
    }
    let mut content_pagination = Map::new();
    if want_body {
        let body = history_body_view(raw.get("body").and_then(Value::as_str).unwrap_or(""), query);
        let (text, pagination) = paginate_text(&body, query.char_offset, query.char_length);
        row["body"] = json!(text);
        content_pagination.insert("body".into(), pagination);
    }
    if want_files {
        shape_pr_files(
            &mut row,
            &mut content_pagination,
            files,
            &states,
            query,
            patch_selector,
            patch_mode,
        );
    }
    if want_discussion || want_inline {
        shape_pr_comments(&mut row, &mut content_pagination, comments, &states, query);
    }
    if want_reviews {
        shape_pr_reviews(&mut row, &mut content_pagination, reviews, &states, query);
    }
    if want_commits {
        shape_pr_commits(
            transport,
            &mut row,
            &mut content_pagination,
            commits,
            &states,
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
    let mut out = json!({"pullRequests":[row]});
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

fn pr_next_menu(
    query: &GhGetHistoryItemQuery,
    content: Option<&Map<String, Value>>,
    patch_mode: &str,
    first_path: Option<&str>,
) -> Value {
    let target = json!({"operation":"pullRequest","owner":query.owner,"repo":query.repo,"number":query.number});
    let mut next = Map::new();
    let call = |content: Value| json!({"tool":"ghGetHistoryItem","query":merge(target.clone(),json!({"content":content})),"confidence":"exact"});
    if !content_flag(content, "body") {
        next.insert("getBody".into(), call(json!({"body":true})));
    }
    if !content_flag(content, "changedFiles") && patch_mode == "none" {
        next.insert("getChangedFiles".into(), call(json!({"changedFiles":true})));
    }
    if patch_mode == "none" {
        if let Some(path) = first_path {
            next.insert(
                "getSelectedPatches".into(),
                call(json!({"patches":{"mode":"selected","files":[path]}})),
            );
        }
        next.insert(
            "getAllPatches".into(),
            call(json!({"patches":{"mode":"all"}})),
        );
    }
    if content.and_then(|v| v.get("comments")).is_none() {
        next.insert(
            "getComments".into(),
            call(json!({"comments":{"discussion":true,"reviewInline":true}})),
        );
    }
    if !content_flag(content, "reviews") {
        next.insert("getReviews".into(), call(json!({"reviews":true})));
    }
    if content.and_then(|v| v.get("commits")).is_none() {
        next.insert("getCommits".into(), call(json!({"commits":{}})));
    }
    Value::Object(next)
}

fn shape_pr_files(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    files: Vec<Value>,
    states: &Map<String, Value>,
    query: &GhGetHistoryItemQuery,
    selector: Option<&Map<String, Value>>,
    patch_mode: &str,
) {
    let mut selected_names = selector
        .and_then(|v| v.get("files"))
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let ranges = selector
        .and_then(|v| v.get("ranges"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|range| {
            let file = range.get("file")?.as_str()?.to_owned();
            let additions = range
                .get("additions")
                .and_then(Value::as_array)
                .map(|lines| lines.iter().filter_map(Value::as_i64).collect::<Vec<_>>());
            let deletions = range
                .get("deletions")
                .and_then(Value::as_array)
                .map(|lines| lines.iter().filter_map(Value::as_i64).collect::<Vec<_>>());
            Some((file, (additions, deletions)))
        })
        .collect::<HashMap<_, _>>();
    for file in ranges.keys() {
        if !selected_names.contains(file) {
            selected_names.push(file.clone());
        }
    }
    let needle = query.match_string.as_deref().map(str::to_lowercase);
    let filtered = files
        .into_iter()
        .filter(|file| {
            let path = str_at(file, "/filename").unwrap_or("");
            (selected_names.is_empty() || selected_names.iter().any(|selected| selected == path))
                && needle.as_ref().is_none_or(|n| {
                    path.to_lowercase().contains(n)
                        || str_at(file, "/patch").is_some_and(|v| v.to_lowercase().contains(n))
                })
        })
        .collect::<Vec<_>>();
    let (slice, mut page) =
        paginate_collection(filtered, query.file_page.or(query.page), query.page_size);
    apply_provider_state(&mut page, states, &["changedFiles"], query);
    let shaped = slice
        .into_iter()
        .map(|mut file| {
            if let Some((additions, deletions)) =
                str_at(&file, "/filename").and_then(|path| ranges.get(path))
            {
                let filtered_patch = octocode_engine::portable::filter_patch(
                    str_at(&file, "/patch").unwrap_or(""),
                    Some(octocode_engine::types::FilterPatchOptions {
                        additions: additions.clone(),
                        deletions: deletions.clone(),
                        ..Default::default()
                    }),
                );
                file["patch"] = Value::String(filtered_patch);
            }
            let mut shaped = shape_file(&file, patch_mode != "none", query);
            if let Some(name) = shaped.as_object_mut().and_then(|v| v.remove("filename")) {
                shaped["path"] = name;
            }
            if let Some(shaped) = shaped.as_object_mut() {
                shaped.remove("previousFilename");
            }
            shaped
        })
        .collect::<Vec<_>>();
    if !shaped.is_empty() {
        row["changedFiles"] = Value::Array(shaped);
    }
    if patch_mode != "none"
        && let Some(patch_page) = row
            .get("changedFiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find_map(|v| v.get("patchPagination"))
            .cloned()
        && patch_page["hasMore"] == true
    {
        pagination.insert("patches".into(), patch_page);
    }
    pagination.insert("changedFiles".into(), page);
}

fn shape_pr_comments(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    comments: Vec<Value>,
    states: &Map<String, Value>,
    query: &GhGetHistoryItemQuery,
) {
    let needle = query.match_string.as_deref().map(str::to_lowercase);
    let mut comments = comments
        .into_iter()
        .filter(|v| {
            needle
                .as_ref()
                .is_none_or(|n| string(v.get("body")).to_lowercase().contains(n))
        })
        .collect::<Vec<_>>();
    comments.sort_by_key(|v| {
        if str_at(v, "/commentType") == Some("review_inline") {
            0
        } else {
            1
        }
    });
    let total_comments = comments.len();
    let inline_comments = comments
        .iter()
        .filter(|v| str_at(v, "/commentType") == Some("review_inline"))
        .count();
    let mut commenters = Vec::<String>::new();
    for author in comments.iter().filter_map(|v| str_at(v, "/author")) {
        if !commenters.iter().any(|v| v == author) {
            commenters.push(author.to_owned());
        }
    }
    let latest = comments
        .iter()
        .filter_map(|v| str_at(v, "/updatedAt").or_else(|| str_at(v, "/createdAt")))
        .max()
        .map(str::to_owned);
    let (slice, mut page) =
        paginate_collection(comments, query.comment_page.or(query.page), query.page_size);
    apply_provider_state(&mut page, states, &["discussion", "inline"], query);
    let mut shaped = Vec::new();
    let mut first_body_page = None;
    for comment in slice {
        let body = history_body_view(&string(comment.get("body")), query);
        let (body, body_page) = paginate_text(&body, query.comment_body_offset, query.char_length);
        if body_page["hasMore"] == true && first_body_page.is_none() {
            first_body_page = Some(body_page.clone());
        }
        let mut item = json!({
            "id": comment["id"], "author": comment["author"],
            "commentType": comment.get("commentType").cloned().unwrap_or(json!("discussion")),
            "path": comment.get("path"), "line": comment.get("line"),
            "body": body, "bodyPagination": body_page,
            "createdAt": comment.get("createdAt"), "updatedAt": comment.get("updatedAt")
        });
        remove_nulls(&mut item);
        shaped.push(item);
    }
    if !shaped.is_empty() {
        row["comments"] = Value::Array(shaped);
        row["reviewSummary"] = json!({"totalComments":total_comments,"inlineComments":inline_comments,"discussionComments":total_comments-inline_comments,"commenters":commenters.iter().take(8).collect::<Vec<_>>(),"commenterCount":commenters.len(),"latestCommentAt":latest,"themes":["discussion"],"countScope":"providerBatch"});
    }
    pagination.insert("comments".into(), page);
    if let Some(page) = first_body_page {
        pagination.insert("commentBody".into(), page);
    }
}

fn shape_pr_reviews(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    reviews: Vec<Value>,
    states: &Map<String, Value>,
    query: &GhGetHistoryItemQuery,
) {
    let needle = query.match_string.as_deref().map(str::to_lowercase);
    let reviews = reviews
        .into_iter()
        .filter(|v| {
            needle
                .as_ref()
                .is_none_or(|n| string(v.get("body")).to_lowercase().contains(n))
        })
        .collect::<Vec<_>>();
    let (slice, mut page) = paginate_collection(reviews, query.review_page, query.page_size);
    apply_provider_state(&mut page, states, &["reviews"], query);
    let mut shaped = Vec::new();
    let mut first_body_page = None;
    for review in slice {
        let body = history_body_view(&string(review.get("body")), query);
        let (body, body_page) = paginate_text(&body, query.char_offset, query.char_length);
        if body_page["hasMore"] == true && first_body_page.is_none() {
            first_body_page = Some(body_page.clone());
        }
        let mut item = json!({
            "id": review["id"].to_string().trim_matches('"'),
            "user": str_at(&review,"/user/login").unwrap_or("unknown"),
            "state": string(review.get("state")),
            "body": (!body.is_empty()).then_some(body),
            "bodyPagination": (!string(review.get("body")).is_empty() && (query.char_offset.unwrap_or(0)>0 || body_page["hasMore"]==true)).then_some(body_page),
            "submittedAt": review.get("submitted_at"), "commitId": review.get("commit_id")
        });
        remove_nulls(&mut item);
        shaped.push(item);
    }
    row["reviews"] = Value::Array(shaped);
    pagination.insert("reviews".into(), page);
    if let Some(page) = first_body_page {
        pagination.insert("reviewBody".into(), page);
    }
}

async fn shape_pr_commits<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    commits: Vec<Value>,
    states: &Map<String, Value>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let include_files = query
        .content
        .as_ref()
        .and_then(|content| content.pointer("/commits/includeFiles"))
        .and_then(Value::as_bool)
        == Some(true);
    let (slice, mut page) =
        paginate_collection(commits, query.commit_page.or(query.page), query.page_size);
    apply_provider_state(&mut page, states, &["commits"], query);
    let mut shaped = Vec::new();
    for item in slice {
        let sha = string(item.get("sha"));
        let mut commit = json!({
            "sha":sha,
            "message":str_at(&item,"/commit/message").unwrap_or(""),
            "author":str_at(&item,"/commit/author/name").unwrap_or("unknown"),
            "date":str_at(&item,"/commit/author/date").unwrap_or("")
        });
        if include_files {
            let (detail, more) = fetch(
                transport,
                &["repos", &query.owner, &query.repo, "commits", &sha],
                &[("per_page", "100".into()), ("page", "1".into())],
                context,
            )
            .await?;
            let files = array(detail.get("files").cloned().unwrap_or(json!([])));
            let (files, files_page_raw) = paginate_collection(files, Some(1), query.page_size);
            let mut files_page = commit_files_pagination(files_page_raw);
            files_page["countScope"] = json!("providerBatch");
            if more {
                files_page["hasMore"] = json!(true);
                files_page["nextFilePage"] = json!(1);
                files_page["nextFileBatch"] = json!(2);
            }
            commit["files"] = Value::Array(
                files
                    .into_iter()
                    .map(|v| shape_file(&v, true, query))
                    .collect(),
            );
            commit["filesPagination"] = files_page;
            attach_diff_continuations(&mut commit, query, ItemOperation::Commit, Some(&sha), true);
        }
        shaped.push(commit);
    }
    row["commits"] = Value::Array(shaped);
    pagination.insert("commits".into(), page);
    Ok(())
}

async fn issue<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let number = query
        .number
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let (raw, _) = fetch(
        transport,
        &["repos", &query.owner, &query.repo, "issues", &number],
        &[],
        context,
    )
    .await?;
    if raw.get("pull_request").is_some_and(|v| !v.is_null()) {
        return Err(validation(&format!(
            "Issue #{} is a pull request; use ghGetHistoryItem operation:\"pullRequest\" with number:{}.",
            number, number
        )));
    }
    let content = query.content.as_ref().and_then(Value::as_object);
    let want_body = content.is_none_or(|v| content_flag(Some(v), "body"));
    let comments = content
        .and_then(|v| v.get("comments"))
        .and_then(Value::as_object);
    let want_comments = content_flag(comments, "discussion");
    let include_bots = content_flag(comments, "includeBots");
    let mut row = json!({
        "number":raw["number"],"title":string(raw.get("title")),
        "state":str_at(&raw,"/state").unwrap_or("open"),"author":str_at(&raw,"/user/login").unwrap_or("unknown"),
        "labels":raw.get("labels").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/name").map(str::to_owned)).collect::<Vec<_>>(),
        "createdAt":string(raw.get("created_at")),"updatedAt":string(raw.get("updated_at")),
        "closedAt":raw.get("closed_at").filter(|v|!v.is_null())
    });
    let mut pagination = Map::new();
    if want_body {
        let body_view = history_body_view(str_at(&raw, "/body").unwrap_or(""), query);
        let (body, page) = paginate_text(&body_view, query.char_offset, query.char_length);
        row["body"] = json!(body);
        if query.char_offset.is_some() || query.char_length.is_some() || page["hasMore"] == true {
            pagination.insert("body".into(), page);
        }
    }
    if want_comments {
        let page_no = query.comment_page.unwrap_or(1);
        let per = query.page_size.unwrap_or(DEFAULT_PAGE_SIZE);
        let (raw_comments, more) = fetch_collection(
            transport,
            &[
                "repos",
                &query.owner,
                &query.repo,
                "issues",
                &number,
                "comments",
            ],
            page_no,
            per,
            context,
        )
        .await?;
        let comments = map_comments(array(raw_comments), "discussion", include_bots);
        let mut shaped = Vec::new();
        let mut body_page = None;
        for comment in comments {
            let body_view = history_body_view(str_at(&comment, "/body").unwrap_or(""), query);
            let (body, page) = paginate_text(&body_view, query.char_offset, query.char_length);
            if page["hasMore"] == true && body_page.is_none() {
                body_page = Some(page.clone())
            }
            let mut c = merge(comment, json!({"body":body,"bodyPagination":page}));
            if let Some(map) = c.as_object_mut()
                && let Some(author) = map.remove("author")
            {
                map.insert("user".into(), author);
            }
            remove_nulls(&mut c);
            shaped.push(c);
        }
        if !shaped.is_empty() {
            row["comments"] = Value::Array(shaped);
        }
        pagination.insert("comments".into(),json!({"currentPage":page_no,"itemsPerPage":per,"totalComments":row["comments"].as_array().map_or(0,Vec::len),"hasMore":more,"nextCommentPage":more.then_some(page_no+1)}));
        if let Some(page) = body_page {
            pagination.insert("commentBody".into(), page);
        }
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    let mut out = json!({"type":"issues","owner":query.owner,"repo":query.repo,"issues":[row],"totalCount":1});
    promote_issue_continuations(&mut out, query);
    Ok(out)
}

async fn commit<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let reference = query
        .reference
        .as_deref()
        .ok_or_else(|| validation("ref is required"))?;
    let batch = query.file_batch.unwrap_or(1);
    let (raw, provider_more) = fetch(
        transport,
        &["repos", &query.owner, &query.repo, "commits", reference],
        &[("per_page", "100".into()), ("page", batch.to_string())],
        context,
    )
    .await?;
    let all_files = array(raw.get("files").cloned().unwrap_or(json!([])));
    let scoped = scope_files(all_files, query.path.as_deref());
    let sha = string(raw.get("sha"));
    let message = str_at(&raw, "/commit/message").unwrap_or("");
    let mut out = json!({
        "type":"commit","owner":query.owner,"repo":query.repo,"ref":reference,"sha":sha,
        "message":message,"messageHeadline":message.lines().next().unwrap_or(message),
        "author":identity(&raw,"author"),"committer":identity(&raw,"committer"),
        "parents":raw.get("parents").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/sha").map(str::to_owned)).collect::<Vec<_>>(),
        "additions":raw.pointer("/stats/additions"),"deletions":raw.pointer("/stats/deletions"),
        "changedFiles":scoped.len(),"changedFilesCountScope":"providerBatch"
    });
    if batch > 1 && scoped.is_empty() {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["providerBatchOutOfRange"]);
    }
    if provider_more {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["providerBatch"]);
    }
    if query.include_diff.unwrap_or(false) {
        let (files, page_raw) = paginate_collection(scoped, query.file_page, query.page_size);
        let mut page = commit_files_pagination(page_raw);
        page["countScope"] = json!("providerBatch");
        page["fileBatch"] = json!(batch);
        if !page["hasMore"].as_bool().unwrap_or(false) && provider_more {
            page["hasMore"] = json!(true);
            page["nextFilePage"] = json!(1);
            page["nextFileBatch"] = json!(batch + 1);
        }
        out["files"] = Value::Array(
            files
                .into_iter()
                .map(|v| shape_file(&v, true, query))
                .collect(),
        );
        out["filesPagination"] = page;
    } else if provider_more {
        out["filesPagination"] = json!({"currentPage":1,"totalPages":1,"itemsPerPage":100,"totalFiles":scoped.len(),"hasMore":true,"nextFilePage":1});
    }
    attach_diff_continuations(&mut out, query, ItemOperation::Commit, Some(&sha), false);
    Ok(out)
}

async fn compare<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let page = query.page.unwrap_or(1);
    let per = query.page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, 100);
    let base = query
        .base
        .as_deref()
        .ok_or_else(|| validation("base is required"))?;
    let head = query
        .head
        .as_deref()
        .ok_or_else(|| validation("head is required"))?;
    let refs = format!("{base}...{head}");
    let (raw, link_more) = fetch(
        transport,
        &["repos", &query.owner, &query.repo, "compare", &refs],
        &[("page", page.to_string()), ("per_page", per.to_string())],
        context,
    )
    .await?;
    let total = usize_at(&raw, "/total_commits");
    let more = link_more || page.saturating_mul(per) < total;
    let (base, head) = compare_identity(&raw, base, head);
    let commits=array(raw.get("commits").cloned().unwrap_or(json!([]))).into_iter().map(|v|json!({
        "sha":v["sha"],"messageHeadline":str_at(&v,"/commit/message").unwrap_or("").lines().next().unwrap_or(""),
        "author":str_at(&v,"/commit/author/name").or_else(||str_at(&v,"/author/login")).unwrap_or("unknown"),"date":str_at(&v,"/commit/author/date").unwrap_or("")
    })).collect::<Vec<_>>();
    let all_files = array(raw.get("files").cloned().unwrap_or(json!([])));
    let file_limit = all_files.len() >= 300;
    let scoped = scope_files(all_files, query.path.as_deref());
    let mut out = json!({"type":"compare","owner":query.owner,"repo":query.repo,"base":base,"head":head,
        "status": raw.get("status"),
        "aheadBy":usize_at(&raw,"/ahead_by"),"behindBy":usize_at(&raw,"/behind_by"),"totalCommits":total,"commits":commits,
        "pagination":{"currentPage":page,"perPage":per,"hasMore":more,"nextPage":more.then_some(page+1)},"isPartial":(more||file_limit).then_some(true)});
    if file_limit {
        out["terminalLimit"] = json!(true);
        out["partialReasons"] = json!(["providerFileLimit"]);
        out["providerLimit"] = json!({"reason":"providerFileLimit","maxFiles":300});
    }
    if !more
        && page > 1
        && let Some(out) = out.as_object_mut()
    {
        out.remove("pagination");
    }
    if query.include_diff.unwrap_or(false) && page == 1 {
        let (files, page) = paginate_collection(scoped, query.file_page, query.page_size);
        out["files"] = Value::Array(
            files
                .into_iter()
                .map(|v| shape_file(&v, true, query))
                .collect(),
        );
        out["filesPagination"] = commit_files_pagination(page);
    } else if page == 1 {
        out["changedFiles"] = json!(scoped.len());
    }
    attach_diff_continuations(&mut out, query, ItemOperation::Compare, None, false);
    Ok(out)
}

async fn fetch_collection<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    segments: &[&str],
    page: usize,
    per: usize,
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    if page == 0 {
        return Ok((json!([]), false));
    }
    fetch(
        transport,
        segments,
        &[("per_page", per.to_string()), ("page", page.to_string())],
        context,
    )
    .await
}

fn content_flag(value: Option<&Map<String, Value>>, key: &str) -> bool {
    value.and_then(|v| v.get(key)).and_then(Value::as_bool) == Some(true)
}
fn collection_page(q: &GhGetHistoryItemQuery, key: &str, default: usize) -> usize {
    q.collection_pages
        .as_ref()
        .and_then(|v| v.get(key))
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(default)
}
fn array(value: Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
fn string(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").to_owned()
}
fn str_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}
fn usize_at(value: &Value, pointer: &str) -> usize {
    value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0) as usize
}
fn nonzero(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64).filter(|v| *v > 0)
}
fn compact(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.into()
    } else {
        format!("{}...", &value[..max - 3])
    }
}

fn map_comments(values: Vec<Value>, kind: &str, include_bots: bool) -> Vec<Value> {
    values.into_iter().filter(|v|include_bots||!is_bot(str_at(v,"/user/login").unwrap_or(""))).map(|v|{
    let mut out=json!({"id":v["id"].to_string().trim_matches('"'),"author":str_at(&v,"/user/login").unwrap_or("unknown"),"body":string(v.get("body")),"createdAt":string(v.get("created_at")),"updatedAt":string(v.get("updated_at")),"commentType":kind,
        "path":v.get("path"),"line":v.get("line").or_else(||v.get("original_line")),"inReplyToId":v.get("in_reply_to_id")});remove_nulls(&mut out);out
}).collect()
}
fn compare_identity(raw: &Value, requested_base: &str, requested_head: &str) -> (String, String) {
    let permalink = raw
        .get("permalink_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let pair = permalink
        .rsplit('/')
        .next()
        .and_then(|tail| tail.split_once("..."));
    let expand = |requested: &str, parsed: Option<&str>| {
        let candidate = parsed.unwrap_or(requested);
        let abbrev = candidate.rsplit(':').next().unwrap_or(candidate);
        if abbrev.len() == 40 {
            return candidate.to_owned();
        }
        let mut known = Vec::new();
        if let Some(sha) = raw.pointer("/base_commit/sha").and_then(Value::as_str) {
            known.push(sha);
        }
        if let Some(sha) = raw
            .pointer("/merge_base_commit/sha")
            .and_then(Value::as_str)
        {
            known.push(sha);
        }
        for commit in raw
            .get("commits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(sha) = commit.get("sha").and_then(Value::as_str) {
                known.push(sha);
            }
        }
        let matches: Vec<_> = known
            .into_iter()
            .filter(|sha| {
                sha.to_ascii_lowercase()
                    .starts_with(&abbrev.to_ascii_lowercase())
            })
            .collect();
        if matches.len() == 1 {
            let prefix = &candidate[..candidate.len().saturating_sub(abbrev.len())];
            format!("{prefix}{}", matches[0])
        } else {
            requested.to_owned()
        }
    };
    (
        expand(requested_base, pair.map(|(base, _)| base)),
        expand(requested_head, pair.map(|(_, head)| head)),
    )
}

fn is_bot(login: &str) -> bool {
    let v = login.to_ascii_lowercase();
    v.ends_with("[bot]")
        || v == "bot"
        || matches!(
            v.as_str(),
            "vercel"
                | "pkg-pr-new"
                | "coderabbitai"
                | "github-actions"
                | "codecov"
                | "changeset-bot"
                | "netlify"
                | "sonarcloud"
                | "socket-security"
        )
}

fn paginate_text(value: &str, offset: Option<usize>, length: Option<usize>) -> (String, Value) {
    let total = value.chars().count();
    let start = offset.unwrap_or(0).min(total);
    let len = length.unwrap_or(DEFAULT_TEXT_WINDOW).clamp(1, 50_000);
    let end = (start + len).min(total);
    let text = value.chars().skip(start).take(end - start).collect();
    (
        text,
        json!({"charOffset":start,"charLength":end-start,"totalChars":total,"hasMore":end<total,"nextCharOffset":(end<total).then_some(end)}),
    )
}

fn history_body_view(value: &str, query: &GhGetHistoryItemQuery) -> String {
    if matches!(query.operation, ItemOperation::PullRequest)
        && query.minify.as_deref() != Some("none")
        && query.match_string.is_none()
    {
        octocode_engine::portable::apply_content_view_minification(value, "history.md")
    } else {
        value.to_owned()
    }
}

fn history_patch_view(value: &str, query: &GhGetHistoryItemQuery) -> String {
    if matches!(query.operation, ItemOperation::PullRequest)
        && query.minify.as_deref() != Some("none")
        && query.match_string.is_none()
    {
        octocode_engine::portable::filter_patch(
            value,
            Some(octocode_engine::types::FilterPatchOptions {
                trim_context: Some(true),
                context_lines: Some(2),
                ..Default::default()
            }),
        )
    } else {
        value.to_owned()
    }
}

fn paginate_collection(
    values: Vec<Value>,
    page: Option<usize>,
    page_size: Option<usize>,
) -> (Vec<Value>, Value) {
    let per = page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, 100);
    let total = values.len();
    let pages = total.div_ceil(per).max(1);
    let current = page.unwrap_or(1).clamp(1, pages);
    let start = (current - 1) * per;
    let more = current < pages;
    (
        values.into_iter().skip(start).take(per).collect(),
        json!({"currentPage":current,"totalPages":pages,"itemsPerPage":per,"totalItems":total,"hasMore":more,"nextPage":more.then_some(current+1)}),
    )
}

fn commit_files_pagination(mut page: Value) -> Value {
    if let Some(map) = page.as_object_mut() {
        if let Some(v) = map.remove("totalItems") {
            map.insert("totalFiles".into(), v);
        }
        if let Some(v) = map.remove("nextPage") {
            map.insert("nextFilePage".into(), v);
        }
    }
    page
}

fn apply_provider_state(
    page: &mut Value,
    states: &Map<String, Value>,
    surfaces: &[&str],
    query: &GhGetHistoryItemQuery,
) {
    page["countScope"] = json!("providerBatch");
    if let Some(cp) = &query.collection_pages {
        page["collectionPages"] = cp.clone();
    }
    if page["hasMore"] != true
        && surfaces.iter().any(|s| {
            states
                .get(*s)
                .and_then(|v| v.get("hasMore"))
                .and_then(Value::as_bool)
                == Some(true)
        })
    {
        page["hasMore"] = json!(true);
        page["nextPage"] = json!(1);
        let mut next = query
            .collection_pages
            .as_ref()
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for surface in surfaces {
            let state = states.get(*surface);
            next.insert(
                (*surface).into(),
                json!(if state
                    .and_then(|v| v.get("hasMore"))
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    state
                        .and_then(|v| v.get("page"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        + 1
                } else {
                    0
                }),
            );
        }
        page["nextCollectionPages"] = Value::Object(next);
    }
}

fn shape_file(file: &Value, include_patch: bool, query: &GhGetHistoryItemQuery) -> Value {
    let mut out = json!({"filename":str_at(file,"/filename").unwrap_or(""),"status":string(file.get("status")),"additions":usize_at(file,"/additions"),"deletions":usize_at(file,"/deletions"),"previousFilename":file.get("previous_filename")});
    if include_patch {
        if let Some(patch) = file.get("patch").and_then(Value::as_str) {
            let patch = history_patch_view(patch, query);
            let (text, page) = paginate_text(
                &patch,
                query.char_offset,
                Some(
                    query
                        .char_length
                        .unwrap_or_else(|| default_patch_window(query.page_size, 1)),
                ),
            );
            out["patch"] = json!(text);
            if query.char_offset.unwrap_or(0) > 0 || page["hasMore"] == true {
                out["patchPagination"] = page;
            }
        } else {
            out["isPartial"] = json!(true);
            out["terminalLimit"] = json!(true);
            out["patchUnavailable"] = json!({"reason":"providerOmittedPatch"});
        }
    }
    remove_nulls(&mut out);
    out
}
fn default_patch_window(page_size: Option<usize>, count: usize) -> usize {
    let files = page_size.unwrap_or(count).clamp(1, count.max(1));
    (DEFAULT_TEXT_WINDOW / files).max(1)
}
fn scope_files(files: Vec<Value>, path: Option<&str>) -> Vec<Value> {
    files
        .into_iter()
        .filter(|v| {
            path.is_none_or(|path| {
                let name = str_at(v, "/filename").unwrap_or("");
                let previous = str_at(v, "/previous_filename").unwrap_or("");
                name == path
                    || previous == path
                    || name.starts_with(
                        if path.ends_with('/') {
                            path.to_owned()
                        } else {
                            format!("{path}/")
                        }
                        .as_str(),
                    )
            })
        })
        .collect()
}

fn identity(raw: &Value, kind: &str) -> Value {
    let p = format!("/commit/{kind}");
    let login = format!("/{kind}/login");
    let mut out = json!({"name":str_at(raw,&format!("{p}/name")).unwrap_or("unknown"),"email":str_at(raw,&format!("{p}/email")).unwrap_or(""),"login":str_at(raw,&login),"date":str_at(raw,&format!("{p}/date"))});
    remove_nulls(&mut out);
    out
}

fn base_public_query(q: &GhGetHistoryItemQuery, operation: ItemOperation) -> Value {
    let mut v = serde_json::to_value(q).unwrap_or_default();
    remove_nulls(&mut v);
    if let Some(m) = v.as_object_mut() {
        m.insert(
            "operation".into(),
            json!(match operation {
                ItemOperation::PullRequest => "pullRequest",
                ItemOperation::Issue => "issue",
                ItemOperation::Commit => "commit",
                ItemOperation::Compare => "compare",
            }),
        );
        m.remove("goal");
        m.remove("reasoning");
        match operation {
            ItemOperation::PullRequest => {
                m.insert(
                    "pageSize".into(),
                    json!(q.page_size.unwrap_or(DEFAULT_PAGE_SIZE)),
                );
                m.insert(
                    "minify".into(),
                    json!(q.minify.as_deref().unwrap_or("standard")),
                );
                if let Some(Value::Object(content)) = m.get_mut("content")
                    && content.get("patches").is_some()
                {
                    content.insert("changedFiles".into(), json!(true));
                }
            }
            ItemOperation::Compare => {
                m.insert("page".into(), json!(q.page.unwrap_or(1)));
                m.insert("filePage".into(), json!(q.file_page.unwrap_or(1)));
                m.insert(
                    "pageSize".into(),
                    json!(q.page_size.unwrap_or(DEFAULT_PAGE_SIZE)),
                );
            }
            _ => {}
        }
    }
    v
}
fn continuation(q: Value) -> Value {
    json!({"tool":"ghGetHistoryItem","query":q,"confidence":"exact"})
}

fn promote_pr_continuations(out: &mut Value, q: &GhGetHistoryItemQuery) {
    let Some(pages) = out
        .pointer_mut("/pullRequests/0/contentPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let mut next = Map::new();
    let mut partial = false;
    for (axis, entry) in pages.iter_mut() {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        partial = true;
        let mut nq = base_public_query(q, ItemOperation::PullRequest);
        if axis == "patches"
            && q.content
                .as_ref()
                .and_then(|value| value.get("changedFiles"))
                .and_then(Value::as_bool)
                != Some(true)
            && let Some(content) = nq.get_mut("content").and_then(Value::as_object_mut)
        {
            content.remove("changedFiles");
        }
        let cursor = match axis.as_str() {
            "body" | "reviewBody" | "patches" => entry
                .get("nextCharOffset")
                .cloned()
                .map(|v| ("charOffset", v)),
            "commentBody" => entry
                .get("nextCharOffset")
                .cloned()
                .map(|v| ("commentBodyOffset", v)),
            "changedFiles" | "filePaths" => entry.get("nextPage").cloned().map(|v| ("filePage", v)),
            "comments" => entry.get("nextPage").cloned().map(|v| ("commentPage", v)),
            "reviews" => entry.get("nextPage").cloned().map(|v| ("reviewPage", v)),
            "commits" => entry.get("nextPage").cloned().map(|v| ("commitPage", v)),
            _ => None,
        };
        if let Some((key, value)) = cursor {
            nq[key] = value;
            if (axis == "changedFiles" || axis == "filePaths" || axis == "reviews")
                && let Some(nq) = nq.as_object_mut()
            {
                nq.remove("charOffset");
            }
            if axis == "comments"
                && let Some(nq) = nq.as_object_mut()
            {
                nq.remove("commentBodyOffset");
            }
            if let Some(cp) = entry.get("nextCollectionPages") {
                nq["collectionPages"] = cp.clone();
            }
            let name = match axis.as_str() {
                "body" => "continueBody",
                "changedFiles" => "nextChangedFilesPage",
                "comments" => "nextCommentsPage",
                "commentBody" => "continueCommentBody",
                "reviews" => "nextReviewsPage",
                "reviewBody" => "continueReviewBody",
                "commits" => "nextCommitsPage",
                "patches" => "continuePatch",
                "filePaths" => "nextFilePathsPage",
                _ => continue,
            };
            next.insert(name.into(), continuation(nq));
        }
    }
    if partial {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["contentPagination"]);
        if !next.is_empty() {
            out["next"] = Value::Object(next);
        }
    }
}

fn promote_issue_continuations(out: &mut Value, q: &GhGetHistoryItemQuery) {
    let Some(pages) = out
        .pointer_mut("/issues/0/contentPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let mut next = Map::new();
    for (axis, entry) in pages.iter() {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let mut nq = base_public_query(q, ItemOperation::Issue);
        let (name, key, cursor, content) = match axis.as_str() {
            "body" => (
                "continueBody",
                "charOffset",
                entry.get("nextCharOffset"),
                json!({"body":true}),
            ),
            "commentBody" => (
                "continueCommentBody",
                "charOffset",
                entry.get("nextCharOffset"),
                json!({"comments":q.content.as_ref().and_then(|v|v.get("comments")).cloned().unwrap_or(json!({"discussion":true}))}),
            ),
            "comments" => (
                "nextCommentsPage",
                "commentPage",
                entry.get("nextCommentPage"),
                json!({"comments":q.content.as_ref().and_then(|v|v.get("comments")).cloned().unwrap_or(json!({"discussion":true}))}),
            ),
            _ => ("", "", None, json!({})),
        };
        if let Some(cursor) = cursor {
            nq[key] = cursor.clone();
            nq["content"] = content;
            if axis == "comments" {
                nq["charOffset"] = json!(0);
            }
            next.insert(name.into(), continuation(nq));
        }
    }
    if !next.is_empty() {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["contentPagination"]);
        out["next"] = Value::Object(next);
    }
}

fn attach_diff_continuations(
    out: &mut Value,
    q: &GhGetHistoryItemQuery,
    operation: ItemOperation,
    resolved_ref: Option<&str>,
    with_why: bool,
) {
    let mut next = Map::new();
    let mut base = if matches!(operation, ItemOperation::Commit) {
        json!({"operation":"commit","owner":q.owner,"repo":q.repo,"ref":resolved_ref.or(q.reference.as_deref()),"includeDiff":true,"fileBatch":q.file_batch,"path":q.path,"filePage":q.file_page.or((!with_why).then_some(1)),"pageSize":q.page_size,"charOffset":q.char_offset,"charLength":q.char_length})
    } else {
        base_public_query(q, operation)
    };
    remove_nulls(&mut base);
    let make = |query: Value, why: &str| {
        if with_why {
            json!({"tool":"ghGetHistoryItem","query":query,"why":why,"confidence":"exact"})
        } else {
            continuation(query)
        }
    };
    if let Some(page) = out.pointer("/pagination/nextPage").cloned() {
        let mut nq = base.clone();
        nq["page"] = page;
        next.insert(
            "nextPage".into(),
            make(
                nq,
                "Continue the comparison commit list. Changed files are returned on page 1.",
            ),
        );
    }
    if let Some(page) = out
        .pointer("/filesPagination/nextFilePage")
        .and_then(Value::as_u64)
        .map(Value::from)
    {
        let mut nq = base.clone();
        nq["filePage"] = page;
        if let Some(nq) = nq.as_object_mut() {
            nq.remove("charOffset");
        }
        if let Some(batch) = out.pointer("/filesPagination/nextFileBatch") {
            nq["fileBatch"] = batch.clone();
        }
        next.insert(
            "nextFilePage".into(),
            make(
                nq,
                "Continue the changed-file list from the beginning of each new patch.",
            ),
        );
    }
    if let Some(offset) = out
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|v| v.pointer("/patchPagination/nextCharOffset"))
        .cloned()
    {
        let mut nq = base;
        nq["charOffset"] = offset;
        next.insert(
            "continuePatch".into(),
            make(nq, "Continue the current patch window."),
        );
    }
    if !next.is_empty() {
        out["next"] = Value::Object(next);
    }
}

fn merge(mut left: Value, right: Value) -> Value {
    if let (Some(l), Some(r)) = (left.as_object_mut(), right.as_object()) {
        l.extend(r.clone());
    }
    left
}
fn remove_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                remove_nulls(v)
            }
        }
        Value::Array(values) => {
            for v in values {
                remove_nulls(v)
            }
        }
        _ => {}
    }
}
fn sanitize_text(value: &str, security: &impl ContentScan) -> Result<String, ProviderError> {
    security
        .sanitize(value, Path::new("github-history-item"))
        .map(|v| v.0)
        .map_err(|(m, _)| ProviderError::new(ProviderErrorKind::Validation, m))
}
fn sanitize_all_strings(
    value: &mut Value,
    security: &impl ContentScan,
) -> Result<(), ProviderError> {
    match value {
        Value::String(v) => *v = sanitize_text(v, security)?,
        Value::Array(values) => {
            for v in values {
                sanitize_all_strings(v, security)?
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                sanitize_all_strings(v, security)?
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ReplacingScan;
    impl ContentScan for ReplacingScan {
        fn sanitize(
            &self,
            text: &str,
            _path: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            Ok((text.replace("secret", "[MASKED]"), Vec::new()))
        }
    }

    #[test]
    fn text_windows_are_unicode_safe() {
        let (value, page) = paginate_text("a🦀b", Some(1), Some(1));
        assert_eq!(value, "🦀");
        assert_eq!(page["nextCharOffset"], 2);
    }
    #[test]
    fn filters_bots() {
        assert!(is_bot("ci[bot]"));
        assert!(is_bot("coderabbitai"));
        assert!(!is_bot("robotics"));
    }
    #[test]
    fn compare_identity_expands_permalink_abbreviations() {
        let raw = json!({
            "permalink_url": "https://github.com/a/b/compare/abc1234...def5678",
            "base_commit": {"sha": "abc1234aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "commits": [{"sha": "def5678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]
        });
        let (base, head) = compare_identity(&raw, "main", "feature");
        assert!(base.starts_with("abc1234"));
        assert!(head.starts_with("def5678"));
    }
    #[test]
    fn missing_identity_is_rejected() {
        let q: GhGetHistoryItemQuery =
            serde_json::from_str(r#"{"operation":"commit","owner":"a","repo":"b"}"#).unwrap();
        assert!(validate(&q).is_err());
    }

    #[test]
    fn sanitizes_every_nested_returned_string() {
        let mut value = json!({
            "title": "secret",
            "nested": [{"body": "a secret value"}],
            "next": {"tool": "secret-tool", "query": {"path": "secret.rs"}}
        });
        sanitize_all_strings(&mut value, &ReplacingScan).unwrap();
        assert_eq!(value["title"], "[MASKED]");
        assert_eq!(value["nested"][0]["body"], "a [MASKED] value");
        assert_eq!(value["next"]["tool"], "[MASKED]-tool");
        assert_eq!(value["next"]["query"]["path"], "[MASKED].rs");
    }

    #[test]
    fn cancellation_is_observed_before_provider_work() {
        let context = RequestContext::with_timeout(std::time::Duration::from_secs(1), 1024);
        context.cancellation.cancel();
        let error = check_context(&context).unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Cancelled);
        assert_eq!(error.message.as_ref(), "request cancelled");
    }

    #[test]
    fn normalized_response_budget_is_enforced() {
        let error = enforce_response_limit(&json!({"body": "abcdefgh"}), 4).unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::ResponseTooLarge);
        assert_eq!(
            error.message.as_ref(),
            "GitHub history item response exceeds 4 bytes"
        );
    }
}
