//! GitHub history *search* — pull requests, issues, and commits.
//!
//! **Cache bypass is intentional.**  This tool uses `&provider.transport`
//! directly rather than the `GitHubProvider<_, GitHubContentCache>` wrapper, so
//! none of the `ConditionalCache` ETag / disk-tier machinery applies.  History
//! search results are inherently mutable (new PRs/issues appear, existing ones
//! are updated, merged, or closed), so caching them would serve stale state;
//! the GitHub API's own rate-limit budget is the right throttle here.
use crate::providers::github::{
    CommitListRequest, CredentialResolver, GitHubTransport, HistoryRequest, IssueListRequest,
    ProviderError, ProviderErrorKind, PullListRequest, RequestContext, quote_search_keyword,
    resolve_date_window,
};
use crate::tools::local_fetch::ContentScan;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhSearchHistoryQuery {
    pub operation: HistoryOperation,
    pub keywords: Option<Vec<String>>,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub author: Option<String>,
    pub assignee: Option<String>,
    pub commenter: Option<String>,
    pub mentions: Option<String>,
    pub label: Option<Vec<String>>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub closed: Option<String>,
    pub comments: Option<String>,
    pub reactions: Option<String>,
    #[serde(rename = "match")]
    pub match_kind: Option<Vec<String>>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub archived: Option<bool>,
    pub state: Option<String>,
    #[serde(rename = "review-requested")]
    pub review_requested: Option<String>,
    #[serde(rename = "reviewed-by")]
    pub reviewed_by: Option<String>,
    pub checks: Option<String>,
    pub review: Option<String>,
    pub head: Option<String>,
    pub base: Option<String>,
    #[serde(rename = "merged-at")]
    pub merged_at: Option<String>,
    pub draft: Option<bool>,
    pub path: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub branch: Option<String>,
    pub committer: Option<String>,
    pub concise: Option<bool>,
    pub include_diff: Option<bool>,
    pub page_size: Option<usize>,
    pub page: Option<usize>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryOperation {
    PullRequest,
    Issue,
    Commit,
}
pub async fn execute<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhSearchHistoryQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, ProviderError> {
    let page = query.page.unwrap_or(1);
    let per = query.page_size.unwrap_or(30).min(100);
    let mut query = query.clone();
    let mut rename_warnings = Vec::new();
    if let (Some(owner), Some(repo)) = (query.owner.as_deref(), query.repo.as_deref()) {
        let (canonical_owner, canonical_repo, renamed, warnings) =
            transport.canonical_owner_repo(owner, repo, context).await?;
        if renamed {
            query.owner = Some(canonical_owner);
            query.repo = Some(canonical_repo);
            rename_warnings = warnings;
        }
    }
    let searching = match query.operation {
        HistoryOperation::Commit => query.keywords.as_ref().is_some_and(|v| !v.is_empty()),
        HistoryOperation::Issue => should_use_search_for_issues(&query),
        HistoryOperation::PullRequest => should_use_search_for_prs(&query),
    };
    if searching && (page - 1).saturating_mul(per) >= 1000 {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "GitHub search page exceeds the 1,000-result search window",
        ));
    }
    let (terms, query_warnings) = build_query_with_warnings(&query)?;
    let request = HistoryRequest {
        query: terms,
        page,
        per_page: per,
        sort: if searching && matches!(query.operation, HistoryOperation::Commit) {
            Some("committer-date".into())
        } else {
            query
                .sort
                .as_ref()
                .filter(|v| v.as_str() != "best-match")
                .cloned()
        },
        order: if searching && matches!(query.operation, HistoryOperation::Commit) {
            Some("desc".into())
        } else {
            query.order.clone()
        },
    };
    let mut result = match query.operation {
        HistoryOperation::Commit if searching => {
            transport.search_commits(&request, context).await?
        }
        HistoryOperation::Commit => {
            let (o, r) = required_repo(&query)?;
            // Invalid-value warnings were already collected by build_query.
            let (since, until) = resolve_commit_window(&query, &mut Vec::new())?;
            let mut listed = transport
                .list_commits_by_committer(
                    &CommitListRequest {
                        owner: o.into(),
                        repo: r.into(),
                        branch: query.branch.clone(),
                        path: query.path.clone(),
                        author: query.author.clone(),
                        since,
                        until,
                        page,
                        per_page: per,
                    },
                    query.committer.as_deref(),
                    context,
                )
                .await?;
            if listed.items.is_empty() && (query.since.is_some() || query.until.is_some()) {
                listed.warnings.push(
                    "since/until matched no commits (GitHub commit listing uses committer date and does not follow renames).".into(),
                );
            }
            listed
        }
        HistoryOperation::Issue if !searching => {
            let (o, r) = required_repo(&query)?;
            transport
                .list_issues(
                    &IssueListRequest {
                        owner: o.into(),
                        repo: r.into(),
                        state: query.state.clone(),
                        assignee: query.assignee.clone(),
                        author: query.author.clone(),
                        mentions: query.mentions.clone(),
                        labels: query.label.clone(),
                        sort: query.sort.clone(),
                        order: query.order.clone(),
                        page,
                        per_page: per,
                    },
                    context,
                )
                .await?
        }
        HistoryOperation::PullRequest if !searching => {
            let (o, r) = required_repo(&query)?;
            transport
                .list_pull_requests(
                    &PullListRequest {
                        owner: o.into(),
                        repo: r.into(),
                        state: query.state.clone(),
                        head: query.head.clone(),
                        base: query.base.clone(),
                        sort: query.sort.clone(),
                        order: query.order.clone(),
                        page,
                        per_page: per,
                    },
                    context,
                )
                .await?
        }
        _ => transport.search_issues(&request, context).await?,
    };
    result.warnings.splice(0..0, rename_warnings);
    result.warnings.extend(query_warnings);
    for item in &mut result.items {
        for key in ["title", "body"] {
            if let Some(text) = item.get(key).and_then(Value::as_str) {
                item[key] = json!(
                    security
                        .sanitize(text, Path::new("github-history"))
                        .map_err(|(m, _)| ProviderError::new(ProviderErrorKind::Validation, m))?
                        .0
                );
            }
        }
        if let Some(text) = item.pointer("/commit/message").and_then(Value::as_str) {
            item["commit"]["message"] = json!(
                security
                    .sanitize(text, Path::new("github-history"))
                    .map_err(|(m, _)| ProviderError::new(ProviderErrorKind::Validation, m))?
                    .0
            );
        }
    }
    let total = if result.listed {
        0
    } else {
        result.total_count.min(1000)
    };
    let pages = total.div_ceil(per).max(1);
    let current_page = if result.listed {
        result.provider_page
    } else {
        page
    };
    let more = if result.listed {
        result.has_more
    } else {
        current_page < pages
    };
    let exact_list_total =
        (result.listed && current_page == 1 && !more).then_some(result.items.len());
    let effective = request.query.clone();
    let mut value = match query.operation {
        HistoryOperation::PullRequest => {
            let rows = result
                .items
                .iter()
                .cloned()
                .map(|item| {
                    if query.concise == Some(true) {
                        concise_row(&item)
                    } else {
                        map_pr(item)
                    }
                })
                .collect::<Vec<_>>();
            let mut v = json!({"type":"pullRequests","pullRequests":rows,"effectiveQuery":effective,"pagination":{"currentPage":current_page,"perPage":per,"hasMore":more,"nextPage":more.then_some(current_page+1)}});
            if !result.listed {
                v["pagination"]["totalPages"] = json!(pages);
                v["pagination"]["totalMatches"] = json!(total);
                v["pagination"]["totalMatchesCapped"] = json!(result.total_count > total);
            } else if let Some(total) = exact_list_total {
                v["pagination"]["totalMatches"] = json!(total);
                v["pagination"]["totalPages"] = json!(1);
            }
            if let Some(number) = v["pullRequests"]
                .get(0)
                .and_then(|x| x.get("number"))
                .and_then(Value::as_u64)
                && let (Some(owner), Some(repo)) = (&query.owner, &query.repo)
            {
                v["next"]["readPr"] = json!({"tool":"ghGetHistoryItem","query":{"operation":"pullRequest","owner":owner,"repo":repo,"number":number,"content":{"body":true,"changedFiles":true,"comments":{"discussion":true}},"pageSize":30,"minify":"standard"},"confidence":"low"});
            }
            v
        }
        HistoryOperation::Issue => {
            let issues = result
                .items
                .iter()
                .cloned()
                .map(|item| {
                    if query.concise == Some(true) {
                        concise_row(&item)
                    } else {
                        map_issue(item)
                    }
                })
                .collect::<Vec<_>>();
            let mut v = json!({"type":"issues","owner":query.owner,"repo":query.repo,"issues":issues,"effectiveQuery":effective,"pagination":{"currentPage":current_page,"perPage":per,"hasMore":more,"nextPage":more.then_some(current_page+1)}});
            if let Some(total) = if result.listed {
                exact_list_total
            } else {
                Some(total)
            } {
                v["totalCount"] = json!(total);
            }
            v
        }
        HistoryOperation::Commit => {
            let mut v = json!({"type":"commits","owner":query.owner,"repo":query.repo,"scope":"defaultBranch","commits":result.items.into_iter().map(if query.keywords.as_ref().is_none_or(Vec::is_empty){map_commit_list}else{map_commit}).collect::<Vec<_>>(),"incompleteResults":result.incomplete_results,"pagination":{"page":page,"perPage":per,"hasMore":more}});
            if let Some(total) = if result.listed {
                exact_list_total
            } else {
                Some(total)
            } {
                v["totalCount"] = json!(total);
            }
            if !result.listed {
                v["pagination"]["totalMatchesCapped"] = json!(result.total_count > total);
            }
            v
        }
    };
    if matches!(query.operation, HistoryOperation::Commit)
        && query.keywords.as_ref().is_none_or(Vec::is_empty)
    {
        let commits = value["commits"].as_array().cloned().unwrap_or_default();
        value = json!({"type":if query.path.as_ref().is_some_and(|p|!p.ends_with('/')){"file"}else{"repo"},"owner":query.owner,"repo":query.repo,"path":query.path,"commits":commits});
        remove_nulls(&mut value);
        if more {
            value["pagination"] =
                json!({"currentPage":page,"perPage":per,"hasMore":true,"nextPage":page+1});
        }
    }
    if query.include_diff == Some(true)
        && matches!(query.operation, HistoryOperation::Commit)
        && let (Some(owner), Some(repo)) = (query.owner.as_deref(), query.repo.as_deref())
        && let Some(commits) = value.get_mut("commits").and_then(Value::as_array_mut)
    {
        for commit in commits.iter_mut().take(per) {
            let Some(sha) = commit.get("sha").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            if let Ok(item) = transport
                .history_item(&["repos", owner, repo, "commits", &sha], &[], context)
                .await
                && let Some(files) = item.value.get("files").cloned()
            {
                commit["files"] = files;
            }
        }
    }
    if !result.warnings.is_empty() {
        value["warnings"] = json!(result.warnings);
    }
    if result.skipped_pull_request_pages > 0 {
        value["skippedPullRequestPages"] = json!(result.skipped_pull_request_pages);
        value["providerPage"] = json!(result.provider_page);
    }
    if matches!(query.operation, HistoryOperation::Issue)
        && let Some(map) = value.as_object_mut()
        && !more
    {
        map.remove("pagination");
    }
    // List mode runs the REST endpoint, not the search terms: reporting them
    // as the effective query would misdescribe what executed.
    if result.listed
        && matches!(
            query.operation,
            HistoryOperation::Issue | HistoryOperation::PullRequest
        )
        && let Some(map) = value.as_object_mut()
    {
        map.remove("effectiveQuery");
    }
    if matches!(query.operation, HistoryOperation::Commit) && more {
        value["pagination"]["nextPage"] = json!(current_page + 1);
    }
    if more {
        let mut next = serde_json::to_value(&query).unwrap_or_default();
        remove_nulls(&mut next);
        next["page"] = json!(current_page + 1);
        // The nextPage continuation validates against the input schema, whose
        // serialization makes the paginated defaulted fields required. Stamp the
        // effective page size so an unset pageSize still yields a valid,
        // directly-executable continuation.
        next["pageSize"] = json!(per);
        value["next"]["nextPage"] =
            json!({"tool":"ghSearchHistory","query":next,"confidence":"exact"});
    }
    remove_nulls(&mut value);
    if result.incomplete_results || (!result.listed && result.total_count > 1000) {
        value["isPartial"] = json!(true);
        value["terminalLimit"] = json!(result.total_count > 1000);
        value["partialReasons"] = json!([if result.total_count > 1000 {
            "providerResultCap"
        } else {
            "providerIncompleteResults"
        }]);
    }
    Ok(value)
}
fn required_repo(q: &GhSearchHistoryQuery) -> Result<(&str, &str), ProviderError> {
    q.owner.as_deref().zip(q.repo.as_deref()).ok_or_else(|| {
        ProviderError::new(ProviderErrorKind::Validation, "owner and repo are required")
    })
}
fn remove_nulls(v: &mut Value) {
    if let Value::Object(m) = v {
        m.retain(|_, v| !v.is_null());
        for v in m.values_mut() {
            remove_nulls(v)
        }
    }
}

fn labels(v: &Value) -> Vec<String> {
    v.get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect()
}
fn map_pr(v: Value) -> Value {
    let merged_at = v
        .get("merged_at")
        .filter(|value| !value.is_null())
        .or_else(|| {
            v.pointer("/pull_request/merged_at")
                .filter(|value| !value.is_null())
        })
        .cloned();
    let state = if merged_at.is_some() {
        json!("merged")
    } else {
        v["state"].clone()
    };
    let mut row = json!({
        "number":v["number"],
        "title":v.get("title").and_then(Value::as_str).unwrap_or(""),
        "state":state,
        "mergedAt":merged_at,
        "author":v.pointer("/user/login").and_then(Value::as_str).unwrap_or(""),
        "labels":labels(&v),
        "createdAt":v.get("created_at").and_then(Value::as_str).unwrap_or(""),
        "commentsCount":v.get("comments").and_then(Value::as_u64).unwrap_or(0)
    });
    remove_nulls(&mut row);
    row
}
fn concise_row(v: &Value) -> Value {
    json!(format!(
        "#{} {}",
        v.get("number").and_then(Value::as_u64).unwrap_or(0),
        v.get("title").and_then(Value::as_str).unwrap_or("")
    ))
}
fn should_use_search_for_issues(q: &GhSearchHistoryQuery) -> bool {
    q.keywords.as_ref().is_some_and(|v| !v.is_empty())
        || q.author.is_some()
        || q.assignee.is_some()
        || q.label.as_ref().is_some_and(|v| !v.is_empty())
        || q.mentions.is_some()
        || q.commenter.is_some()
        || q.reactions.is_some()
        || q.comments.is_some()
        || q.created.is_some()
        || q.updated.is_some()
        || q.closed.is_some()
        || q.match_kind.as_ref().is_some_and(|v| !v.is_empty())
        || matches!(q.sort.as_deref(), Some("comments" | "reactions"))
}
fn should_use_search_for_prs(q: &GhSearchHistoryQuery) -> bool {
    // The REST list endpoint needs owner+repo; anything broader is search.
    q.owner.is_none()
        || q.repo.is_none()
        || should_use_search_for_issues(q)
        || q.draft.is_some()
        || q.reviewed_by.is_some()
        || q.review_requested.is_some()
        || q.checks.is_some()
        || q.review.is_some()
        || q.head.is_some()
        || q.base.is_some()
        || q.merged_at.is_some()
        || q.state.as_deref() == Some("merged")
}
fn map_issue(v: Value) -> Value {
    json!({"number":v["number"],"title":v["title"],"state":v["state"],"author":v.pointer("/user/login"),"labels":labels(&v),"createdAt":v["created_at"],"updatedAt":v["updated_at"]})
}
fn map_commit(v: Value) -> Value {
    let message = v
        .pointer("/commit/message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    // No per-row html_url: it is owner/repo/commit/sha, all already in the row
    // and its envelope (issue and PR rows omit it too).
    json!({"sha":v["sha"],"messageHeadline":message,"date":v.pointer("/commit/author/date"),"author":{"name":v.pointer("/commit/author/name"),"email":v.pointer("/commit/author/email"),"login":v.pointer("/author/login")}})
}

#[cfg(test)]
fn build_query(q: &GhSearchHistoryQuery) -> Result<String, ProviderError> {
    build_query_with_warnings(q).map(|(terms, _)| terms)
}

/// Resolves `since`/`until`, collecting invalid-value warnings and rejecting
/// an inverted window (since after until) as a validation error.
fn resolve_commit_window(
    q: &GhSearchHistoryQuery,
    warnings: &mut Vec<String>,
) -> Result<(Option<String>, Option<String>), ProviderError> {
    let since = q.since.as_deref().map(resolve_date_window);
    let until = q.until.as_deref().map(resolve_date_window);
    if let (Some(since), Some(until)) = (&since, &until)
        && since.is_after(until)
    {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "since ({}) is after until ({}); swap them or widen the window",
                q.since.as_deref().unwrap_or_default().trim(),
                q.until.as_deref().unwrap_or_default().trim()
            ),
        ));
    }
    let mut values = [None, None];
    for (slot, window) in values.iter_mut().zip([since, until]) {
        if let Some(window) = window {
            warnings.extend(window.warning);
            *slot = window.value;
        }
    }
    let [since, until] = values;
    Ok((since, until))
}

fn build_query_with_warnings(
    q: &GhSearchHistoryQuery,
) -> Result<(String, Vec<String>), ProviderError> {
    let mut warnings = Vec::new();
    let mut out = q
        .keywords
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|keyword| quote_search_keyword(&keyword))
        .collect::<Vec<_>>();
    let push = |out: &mut Vec<String>, k: &str, v: Option<&str>| {
        if let Some(v) = v {
            out.push(format!("{k}:{v}"));
        }
    };
    match q.operation {
        HistoryOperation::Commit => {
            let (o, r) = required_repo(q)?;
            out.push(format!("repo:{o}/{r}"));
            for (field, value) in [
                ("author", q.author.as_deref()),
                ("committer", q.committer.as_deref()),
            ] {
                if let Some(value) = value {
                    let key = if value.contains('@') {
                        format!("{field}-email")
                    } else {
                        field.into()
                    };
                    out.push(format!("{key}:{value}"));
                }
            }
            let (since, until) = resolve_commit_window(q, &mut warnings)?;
            match (since.as_deref(), until.as_deref()) {
                (Some(since), Some(until)) => {
                    out.push(format!("committer-date:{since}..{until}"));
                }
                (Some(since), None) => out.push(format!("committer-date:>={since}")),
                (None, Some(until)) => out.push(format!("committer-date:<={until}")),
                (None, None) => {}
            }
        }
        HistoryOperation::PullRequest | HistoryOperation::Issue => {
            if let Some(v) = &q.match_kind {
                out.push(format!("in:{}", v.join(",")));
            }
            out.push(
                if matches!(q.operation, HistoryOperation::PullRequest) {
                    "is:pr"
                } else {
                    "is:issue"
                }
                .into(),
            );
            match (q.owner.as_deref(), q.repo.as_deref(), q.operation) {
                (Some(o), Some(r), _) => out.push(format!("repo:{o}/{r}")),
                // Pull-request search is cross-repo capable (contract: owner and
                // repo optional); issue search stays repository-scoped.
                (Some(o), None, HistoryOperation::PullRequest) => out.push(format!("user:{o}")),
                (None, _, HistoryOperation::PullRequest) => {}
                _ => {
                    required_repo(q)?;
                }
            }
            if let Some(state) = &q.state {
                out.push(format!("is:{state}"));
            }
            if let Some(draft) = q.draft {
                out.push(if draft { "is:draft" } else { "-is:draft" }.into());
            }
            for (k, v) in [
                ("author", q.author.as_deref()),
                ("assignee", q.assignee.as_deref()),
                ("mentions", q.mentions.as_deref()),
                ("commenter", q.commenter.as_deref()),
                ("reviewed-by", q.reviewed_by.as_deref()),
                ("review-requested", q.review_requested.as_deref()),
                ("head", q.head.as_deref()),
                ("base", q.base.as_deref()),
                ("created", q.created.as_deref()),
                ("updated", q.updated.as_deref()),
                ("merged", q.merged_at.as_deref()),
                ("closed", q.closed.as_deref()),
                ("comments", q.comments.as_deref()),
                ("reactions", q.reactions.as_deref()),
                ("review", q.review.as_deref()),
            ] {
                push(&mut out, k, v);
            }
            if let Some(labels) = &q.label {
                for label in labels {
                    out.push(format!("label:\"{label}\""));
                }
            }
            if let Some(archived) = q.archived {
                out.push(format!("archived:{archived}"));
            }
            push(&mut out, "status", q.checks.as_deref());
        }
    }
    Ok((out.join(" "), warnings))
}

fn map_commit_list(v: Value) -> Value {
    let full = v
        .pointer("/commit/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    let headline = full.lines().next().unwrap_or("");
    let body = full.split_once('\n').map(|(_, rest)| rest.trim_start());
    let truncated = body.is_some_and(|text| text.chars().count() > 500);
    let body = body.map(|text| text.chars().take(500).collect::<String>());
    let author_login = v.pointer("/author/login").and_then(Value::as_str);
    let committer_login = v.pointer("/committer/login").and_then(Value::as_str);
    let same = author_login == committer_login
        || committer_login == Some("web-flow")
        || v.pointer("/commit/committer/name") == v.pointer("/commit/author/name");
    let mut row = json!({
        "sha": v["sha"],
        "date": v.pointer("/commit/author/date"),
        "messageHeadline": headline,
        "author": {
            "name": v.pointer("/commit/author/name"),
            "email": v.pointer("/commit/author/email"),
            "login": author_login
        }
    });
    if let Some(body) = body.filter(|text| !text.is_empty()) {
        row["messageBody"] = json!(body);
        if truncated {
            row["messageTruncated"] = json!(true);
        }
    }
    if !same {
        row["committer"] = json!({
            "name": v.pointer("/commit/committer/name"),
            "email": v.pointer("/commit/committer/email").and_then(Value::as_str).unwrap_or(""),
            "login": committer_login
        });
    }
    row
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_issue_qualifier_order() {
        let q: GhSearchHistoryQuery=serde_json::from_str(r#"{"operation":"issue","owner":"a","repo":"b","keywords":["x"],"state":"closed","match":["title"],"label":["bug"]}"#).expect("GitHub history search test data should be valid");
        assert_eq!(
            build_query(&q).expect("GitHub history search test data should be valid"),
            "x in:title is:issue repo:a/b is:closed label:\"bug\""
        );
        let archived: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"issue","owner":"a","repo":"b","keywords":["x"],"archived":true}"#,
        )
        .expect("valid");
        assert!(
            build_query(&archived)
                .expect("valid")
                .ends_with("archived:true")
        );
    }
    #[test]
    fn quotes_multiword_history_keywords() {
        let q: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"issue","owner":"a","repo":"b","keywords":["fix login"]}"#,
        )
        .expect("GitHub history search test data should be valid");
        assert!(
            build_query(&q)
                .expect("GitHub history search test data should be valid")
                .starts_with("\"fix login\"")
        );
        assert!(should_use_search_for_issues(&q));
        let listed: GhSearchHistoryQuery =
            serde_json::from_str(r#"{"operation":"issue","owner":"a","repo":"b"}"#)
                .expect("GitHub history search test data should be valid");
        assert!(!should_use_search_for_issues(&listed));
    }
    #[test]
    fn commit_search_uses_email_and_committer_date() {
        let q: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"commit","owner":"a","repo":"b","keywords":["fix"],"author":"dev@example.com","since":"2026-01-01T00:00:00Z"}"#,
        )
        .expect("GitHub history search test data should be valid");
        let query = build_query(&q).expect("GitHub history search test data should be valid");
        assert!(query.contains("author-email:dev@example.com"));
        assert!(query.contains("committer-date:>=2026-01-01T00:00:00Z"));
    }
    #[test]
    fn rejects_unscoped_commit() {
        let q: GhSearchHistoryQuery = serde_json::from_str(r#"{"operation":"commit","owner":"a"}"#)
            .expect("GitHub history search test data should be valid");
        assert!(build_query(&q).is_err());
    }

    #[test]
    fn pull_request_rows_normalize_merged_state_from_list_and_search_shapes() {
        let listed = map_pr(json!({
            "number":1,
            "title":"listed",
            "state":"closed",
            "merged_at":"2026-09-20T10:00:00Z",
            "user":{"login":"dev"},
            "labels":[],
            "created_at":"2026-09-19T10:00:00Z",
            "comments":2
        }));
        assert_eq!(listed["state"], "merged");
        assert_eq!(listed["mergedAt"], "2026-09-20T10:00:00Z");
        assert_eq!(listed["author"], "dev");
        assert_eq!(listed["commentsCount"], 2);

        let searched = map_pr(json!({
            "number":2,
            "title":"searched",
            "state":"closed",
            "pull_request":{"merged_at":"2026-09-20T11:00:00Z"},
            "user":{"login":"dev"},
            "labels":[],
            "created_at":"2026-09-19T11:00:00Z",
            "comments":0
        }));
        assert_eq!(searched["state"], "merged");
        assert_eq!(searched["mergedAt"], "2026-09-20T11:00:00Z");

        let closed = map_pr(json!({
            "number":3,
            "title":"closed",
            "state":"closed",
            "merged_at":null,
            "user":{"login":"dev"},
            "labels":[],
            "created_at":"2026-09-19T12:00:00Z",
            "comments":0
        }));
        assert_eq!(closed["state"], "closed");
        assert!(closed.get("mergedAt").is_none());
    }

    fn parse(json: &str) -> GhSearchHistoryQuery {
        serde_json::from_str(json).expect("GitHub history search test data should be valid")
    }

    #[test]
    fn pull_request_search_allows_cross_repo_and_owner_scopes() {
        let both = build_query(&parse(
            r#"{"operation":"pullRequest","owner":"a","repo":"b","keywords":["x"]}"#,
        ))
        .expect("scoped");
        assert!(both.contains("repo:a/b"), "{both}");
        let owner = build_query(&parse(
            r#"{"operation":"pullRequest","owner":"a","keywords":["x"]}"#,
        ))
        .expect("owner-scoped PR search");
        assert!(
            owner.contains("user:a") && !owner.contains("repo:"),
            "{owner}"
        );
        let global = build_query(&parse(r#"{"operation":"pullRequest","keywords":["x"]}"#))
            .expect("cross-repo PR search");
        assert!(
            !global.contains("repo:") && !global.contains("user:"),
            "{global}"
        );
        assert!(!global.contains("archived:"), "{global}");
        // Without a full repo scope the REST list endpoint is unusable, so the
        // PR path must route through search.
        assert!(should_use_search_for_prs(&parse(
            r#"{"operation":"pullRequest","owner":"a"}"#
        )));
        assert!(!should_use_search_for_prs(&parse(
            r#"{"operation":"pullRequest","owner":"a","repo":"b"}"#
        )));
        // Issues still require the repository scope.
        assert!(build_query(&parse(r#"{"operation":"issue","keywords":["x"]}"#)).is_err());
    }

    #[test]
    fn commit_search_surfaces_invalid_date_warnings() {
        let q = parse(
            r#"{"operation":"commit","owner":"a","repo":"b","keywords":["fix"],"since":"yesterday-ish"}"#,
        );
        let (terms, warnings) = build_query_with_warnings(&q).expect("valid");
        assert!(!terms.contains("committer-date"), "{terms}");
        assert!(
            warnings.iter().any(|w| w.contains("yesterday-ish")),
            "{warnings:?}"
        );
    }

    #[test]
    fn inverted_since_until_is_a_validation_error() {
        let q = parse(
            r#"{"operation":"commit","owner":"a","repo":"b","keywords":["fix"],"since":"2026-05-01","until":"2026-01-01"}"#,
        );
        let error = build_query(&q).expect_err("since after until");
        assert_eq!(error.kind, ProviderErrorKind::Validation);
        assert!(error.message.contains("since"), "{}", error.message);
        let listed = parse(
            r#"{"operation":"commit","owner":"a","repo":"b","since":"2026-05-01","until":"2026-01-01"}"#,
        );
        assert!(build_query(&listed).is_err());
    }
}
