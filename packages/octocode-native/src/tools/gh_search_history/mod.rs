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
    PullRequests,
    Issues,
    Commits,
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
    if let (Some(owner), Some(repo)) = (query.owner.clone(), query.repo.clone()) {
        let (canonical_owner, canonical_repo, renamed, warnings) = transport
            .canonical_owner_repo(&owner, &repo, context)
            .await?;
        if renamed {
            query.owner = Some(canonical_owner);
            query.repo = Some(canonical_repo);
            rename_warnings = warnings;
        }
    }
    let searching = match query.operation {
        HistoryOperation::Commits => query.keywords.as_ref().is_some_and(|v| !v.is_empty()),
        HistoryOperation::Issues => should_use_search_for_issues(&query),
        HistoryOperation::PullRequests => should_use_search_for_prs(&query),
    };
    if searching && (page - 1).saturating_mul(per) >= 1000 {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "GitHub search page exceeds the 1,000-result search window",
        ));
    }
    let terms = build_query(&query)?;
    let request = HistoryRequest {
        query: terms,
        page,
        per_page: per,
        sort: if searching && matches!(query.operation, HistoryOperation::Commits) {
            Some("committer-date".into())
        } else {
            query
                .sort
                .as_ref()
                .filter(|v| v.as_str() != "best-match")
                .cloned()
        },
        order: if searching && matches!(query.operation, HistoryOperation::Commits) {
            Some("desc".into())
        } else {
            query.order.clone()
        },
    };
    let mut result = match query.operation {
        HistoryOperation::Commits if searching => {
            transport.search_commits(&request, context).await?
        }
        HistoryOperation::Commits => {
            let (o, r) = required_repo(&query)?;
            let since = query.since.as_deref().map(resolve_date_window);
            let until = query.until.as_deref().map(resolve_date_window);
            let mut listed = transport
                .list_commits(
                    &CommitListRequest {
                        owner: o.into(),
                        repo: r.into(),
                        branch: query.branch.clone(),
                        path: query.path.clone(),
                        author: query.author.clone(),
                        since: since.as_ref().and_then(|w| w.value.clone()),
                        until: until.as_ref().and_then(|w| w.value.clone()),
                        page,
                        per_page: per,
                    },
                    context,
                )
                .await?;
            for window in [since, until].into_iter().flatten() {
                if let Some(warning) = window.warning {
                    listed.warnings.push(warning);
                }
            }
            if listed.items.is_empty() && (query.since.is_some() || query.until.is_some()) {
                listed.warnings.push(
                    "since/until matched no commits (GitHub commit listing uses committer date and does not follow renames).".into(),
                );
            }
            listed
        }
        HistoryOperation::Issues if !searching => {
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
        HistoryOperation::PullRequests if !searching => {
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
        result.total_count
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
        result.total_count > result.items.len()
    } else {
        current_page < pages
    };
    let effective = request.query.clone();
    let mut value = match query.operation {
        HistoryOperation::PullRequests => {
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
            let mut v = json!({"pullRequests":rows,"effectiveQuery":effective,"pagination":{"currentPage":current_page,"totalPages":pages,"perPage":per,"totalMatches":total,"totalMatchesCapped":!result.listed && result.total_count>total,"hasMore":more,"nextPage":more.then_some(current_page+1)}});
            if let Some(number) = v["pullRequests"]
                .get(0)
                .and_then(|x| x.get("number"))
                .and_then(Value::as_u64)
                && let (Some(owner), Some(repo)) = (&query.owner, &query.repo)
            {
                v["next"]["readPr"] = json!({"tool":"ghGetHistoryItem","query":{"operation":"pullRequest","owner":owner,"repo":repo,"number":number,"content":{"body":true,"changedFiles":true,"comments":{"discussion":true}}},"confidence":"low"});
            }
            v
        }
        HistoryOperation::Issues => {
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
            json!({"type":"issues","owner":query.owner,"repo":query.repo,"issues":issues,"totalCount":total,"effectiveQuery":effective,"pagination":{"currentPage":current_page,"perPage":per,"hasMore":more,"nextPage":more.then_some(current_page+1)}})
        }
        HistoryOperation::Commits => {
            json!({"type":"commits","owner":query.owner,"repo":query.repo,"scope":"defaultBranch","commits":result.items.into_iter().map(if query.keywords.as_ref().is_none_or(Vec::is_empty){map_commit_list}else{map_commit}).collect::<Vec<_>>(),"totalCount":total,"incompleteResults":result.incomplete_results,"pagination":{"page":page,"perPage":per,"hasMore":more,"totalMatchesCapped":result.total_count>total}})
        }
    };
    if matches!(query.operation, HistoryOperation::Commits)
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
        && matches!(query.operation, HistoryOperation::Commits)
        && let (Some(owner), Some(repo)) = (query.owner.clone(), query.repo.clone())
        && let Some(commits) = value.get_mut("commits").and_then(Value::as_array_mut)
    {
        for commit in commits.iter_mut().take(per) {
            let Some(sha) = commit.get("sha").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            if let Ok(item) = transport
                .history_item(&["repos", &owner, &repo, "commits", &sha], &[], context)
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
    if matches!(query.operation, HistoryOperation::Issues)
        && !more
        && !result.listed
        && let Some(map) = value.as_object_mut()
    {
        map.remove("pagination");
    }
    if matches!(query.operation, HistoryOperation::Commits) && more {
        value["pagination"]["nextPage"] = json!(current_page + 1);
    }
    if more {
        let mut next = serde_json::to_value(&query).unwrap_or_default();
        remove_nulls(&mut next);
        next["page"] = json!(current_page + 1);
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
    json!({"number":v["number"],"title":v["title"],"state":v["state"],"author":v.pointer("/user/login"),"labels":labels(&v),"createdAt":v["created_at"],"commentsCount":v["comments"]})
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
    should_use_search_for_issues(q)
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
    json!({"sha":v["sha"],"url":v["html_url"],"messageHeadline":message,"date":v.pointer("/commit/author/date"),"author":{"name":v.pointer("/commit/author/name"),"email":v.pointer("/commit/author/email"),"login":v.pointer("/author/login")}})
}

fn build_query(q: &GhSearchHistoryQuery) -> Result<String, ProviderError> {
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
        HistoryOperation::Commits => {
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
            let since = q.since.as_deref().map(resolve_date_window);
            let until = q.until.as_deref().map(resolve_date_window);
            match (
                since.as_ref().and_then(|w| w.value.as_deref()),
                until.as_ref().and_then(|w| w.value.as_deref()),
            ) {
                (Some(since), Some(until)) => {
                    out.push(format!("committer-date:{since}..{until}"));
                }
                (Some(since), None) => out.push(format!("committer-date:>={since}")),
                (None, Some(until)) => out.push(format!("committer-date:<={until}")),
                (None, None) => {}
            }
        }
        HistoryOperation::PullRequests | HistoryOperation::Issues => {
            if let Some(v) = &q.match_kind {
                out.push(format!("in:{}", v.join(",")));
            }
            out.push(
                if matches!(q.operation, HistoryOperation::PullRequests) {
                    "is:pr"
                } else {
                    "is:issue"
                }
                .into(),
            );
            let (o, r) = required_repo(q)?;
            out.push(format!("repo:{o}/{r}"));
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
            out.push(format!("archived:{}", q.archived.unwrap_or(false)));
            push(&mut out, "status", q.checks.as_deref());
        }
    }
    Ok(out.join(" "))
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
        let q: GhSearchHistoryQuery=serde_json::from_str(r#"{"operation":"issues","owner":"a","repo":"b","keywords":["x"],"state":"closed","match":["title"],"label":["bug"]}"#).expect("GitHub history search test data should be valid");
        assert_eq!(
            build_query(&q).expect("GitHub history search test data should be valid"),
            "x in:title is:issue repo:a/b is:closed label:\"bug\" archived:false"
        );
    }
    #[test]
    fn quotes_multiword_history_keywords() {
        let q: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"issues","owner":"a","repo":"b","keywords":["fix login"]}"#,
        )
        .expect("GitHub history search test data should be valid");
        assert!(
            build_query(&q)
                .expect("GitHub history search test data should be valid")
                .starts_with("\"fix login\"")
        );
        assert!(should_use_search_for_issues(&q));
        let listed: GhSearchHistoryQuery =
            serde_json::from_str(r#"{"operation":"issues","owner":"a","repo":"b"}"#)
                .expect("GitHub history search test data should be valid");
        assert!(!should_use_search_for_issues(&listed));
    }
    #[test]
    fn commit_search_uses_email_and_committer_date() {
        let q: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"commits","owner":"a","repo":"b","keywords":["fix"],"author":"dev@example.com","since":"2026-01-01T00:00:00Z"}"#,
        )
        .expect("GitHub history search test data should be valid");
        let query = build_query(&q).expect("GitHub history search test data should be valid");
        assert!(query.contains("author-email:dev@example.com"));
        assert!(query.contains("committer-date:>=2026-01-01T00:00:00Z"));
    }
    #[test]
    fn rejects_unscoped_commit() {
        let q: GhSearchHistoryQuery =
            serde_json::from_str(r#"{"operation":"commits","owner":"a"}"#)
                .expect("GitHub history search test data should be valid");
        assert!(build_query(&q).is_err());
    }
}
