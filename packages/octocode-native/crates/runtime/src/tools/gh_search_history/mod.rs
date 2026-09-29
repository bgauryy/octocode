//! GitHub history *search* — pull requests, issues, and commits.
//!
//! **Cache bypass is intentional.**  This tool uses `&provider.transport`
//! directly rather than the `GitHubProvider<_, GitHubContentCache>` wrapper, so
//! none of the `ConditionalCache` ETag / disk-tier machinery applies.  History
//! search results are inherently mutable (new PRs/issues appear, existing ones
//! are updated, merged, or closed), so caching them would serve stale state;
//! the GitHub API's own rate-limit budget is the right throttle here.
pub use crate::contracts::tool_types::{
    GhSearchHistoryQuery, GhSearchHistoryQueryMatchItem, GhSearchHistoryQueryOwner,
    GhSearchHistoryQueryRepo,
};
use crate::providers::github::{
    CommitListRequest, CredentialResolver, GitHubTransport, HistoryRequest, ProviderError,
    ProviderErrorKind, PullListRequest, RequestContext, SearchName, quote_search_keyword,
    resolve_date_window, validate_qualifier_value, validate_search_name,
};
use crate::security::scan::ContentScan;
use crate::tools::result::remove_null_fields;
use serde_json::{Value, json};
use std::path::Path;

/// The operation discriminant of a [`GhSearchHistoryQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryOperation {
    PullRequest,
    Issue,
    Commit,
}

/// Reads a string field shared by some operations; others lack it.
macro_rules! history_str_fields {
    ($($field:ident: $($variant:ident)|+;)+) => {
        $(pub fn $field(&self) -> Option<&str> {
            match self {
                $(Self::$variant { $field, .. } => $field.as_deref(),)+
                #[allow(unreachable_patterns)]
                _ => None,
            }
        })+
    };
}

/// Reads an enum field as its wire string; operations without it yield `None`.
macro_rules! history_enum_fields {
    ($($field:ident: $($variant:ident)|+;)+) => {
        $(pub fn $field(&self) -> Option<String> {
            match self {
                $(Self::$variant { $field, .. } => $field.as_ref().map(ToString::to_string),)+
                #[allow(unreachable_patterns)]
                _ => None,
            }
        })+
    };
}

/// Operation-independent views over the generated wire query.
impl GhSearchHistoryQuery {
    pub fn operation(&self) -> HistoryOperation {
        match self {
            Self::PullRequest { .. } => HistoryOperation::PullRequest,
            Self::Issue { .. } => HistoryOperation::Issue,
            Self::Commit { .. } => HistoryOperation::Commit,
        }
    }
    pub fn owner(&self) -> Option<&str> {
        match self {
            Self::PullRequest { owner, .. } => owner.as_deref().map(String::as_str),
            Self::Issue { owner, .. } | Self::Commit { owner, .. } => Some(owner.as_str()),
        }
    }
    pub fn repo(&self) -> Option<&str> {
        match self {
            Self::PullRequest { repo, .. } => repo.as_deref().map(String::as_str),
            Self::Issue { repo, .. } | Self::Commit { repo, .. } => Some(repo.as_str()),
        }
    }
    /// Points the query at a repository's canonical (post-rename) name.
    /// Names the contract would reject leave the query unchanged.
    pub fn set_scope(&mut self, new_owner: &str, new_repo: &str) {
        let (Ok(new_owner), Ok(new_repo)) = (
            new_owner.parse::<GhSearchHistoryQueryOwner>(),
            new_repo.parse::<GhSearchHistoryQueryRepo>(),
        ) else {
            return;
        };
        match self {
            Self::PullRequest { owner, repo, .. } => {
                *owner = Some(new_owner);
                *repo = Some(new_repo);
            }
            Self::Issue { owner, repo, .. } | Self::Commit { owner, repo, .. } => {
                *owner = new_owner;
                *repo = new_repo;
            }
        }
    }
    pub fn keywords(&self) -> Vec<&str> {
        match self {
            Self::PullRequest { keywords, .. } | Self::Issue { keywords, .. } => {
                keywords.iter().map(String::as_str).collect()
            }
            Self::Commit { keywords, .. } => keywords.iter().map(|k| k.as_str()).collect(),
        }
    }
    pub fn label(&self) -> &[String] {
        match self {
            Self::PullRequest { label, .. } | Self::Issue { label, .. } => label,
            Self::Commit { .. } => &[],
        }
    }
    pub fn match_kinds(&self) -> &[GhSearchHistoryQueryMatchItem] {
        match self {
            Self::PullRequest { match_, .. } | Self::Issue { match_, .. } => match_,
            Self::Commit { .. } => &[],
        }
    }
    pub fn archived(&self) -> Option<bool> {
        match self {
            Self::PullRequest { archived, .. } | Self::Issue { archived, .. } => *archived,
            Self::Commit { .. } => None,
        }
    }
    pub fn draft(&self) -> Option<bool> {
        match self {
            Self::PullRequest { draft, .. } => *draft,
            _ => None,
        }
    }
    pub fn concise(&self) -> Option<bool> {
        match self {
            Self::PullRequest { concise, .. } | Self::Issue { concise, .. } => *concise,
            Self::Commit { .. } => None,
        }
    }
    pub fn page(&self) -> Option<usize> {
        match self {
            Self::PullRequest { page, .. } | Self::Issue { page, .. } => page.map(usize_of),
            Self::Commit { page, .. } => Some(usize_of(*page)),
        }
    }
    pub fn page_size(&self) -> Option<usize> {
        match self {
            Self::PullRequest { page_size, .. }
            | Self::Issue { page_size, .. }
            | Self::Commit { page_size, .. } => page_size.as_ref().map(|size| usize_of(size.0)),
        }
    }
    history_str_fields! {
        author: PullRequest | Issue | Commit;
        assignee: PullRequest | Issue;
        commenter: PullRequest | Issue;
        mentions: PullRequest | Issue;
        created: PullRequest | Issue;
        updated: PullRequest | Issue;
        closed: PullRequest | Issue;
        comments: PullRequest | Issue;
        reactions: PullRequest | Issue;
        review_requested: PullRequest;
        reviewed_by: PullRequest;
        head: PullRequest;
        base: PullRequest;
        merged_at: PullRequest;
        path: Commit;
        since: Commit;
        until: Commit;
        branch: Commit;
        committer: Commit;
    }
    history_enum_fields! {
        sort: PullRequest | Issue;
        order: PullRequest | Issue;
        state: PullRequest | Issue;
        checks: PullRequest;
        review: PullRequest;
    }
}

fn usize_of(value: std::num::NonZeroU64) -> usize {
    usize::try_from(value.get()).unwrap_or(usize::MAX)
}
pub async fn execute<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhSearchHistoryQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, ProviderError> {
    let page = query.page().unwrap_or(1);
    let per = query.page_size().unwrap_or(30).min(100);
    let mut query = query.clone();
    // Issues always use search: GitHub's REST /issues list interleaves pull
    // requests, so filtering them out of provider pages underfills pages and
    // makes page numbers skip. `is:issue` search pages count issues only.
    let searching = match query.operation() {
        HistoryOperation::Commit => !query.keywords().is_empty(),
        HistoryOperation::Issue => true,
        HistoryOperation::PullRequest => should_use_search_for_prs(&query),
    };
    let mut rename_warnings = Vec::new();
    // Only the search API ignores renames; REST list endpoints follow the
    // repository redirect, so list paths skip the extra /repos lookup.
    if searching && let (Some(owner), Some(repo)) = (query.owner(), query.repo()) {
        let (canonical_owner, canonical_repo, renamed, warnings) =
            transport.canonical_owner_repo(owner, repo, context).await?;
        if renamed {
            query.set_scope(&canonical_owner, &canonical_repo);
            rename_warnings = warnings;
        }
    }
    if searching && (page - 1).saturating_mul(per) >= 1000 {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "GitHub search page exceeds the 1,000-result search window",
        )
        .with_reason(crate::providers::github::ProviderErrorReason::SearchWindowExceeded));
    }
    let (terms, query_warnings) = build_query_with_warnings(&query)?;
    let request = HistoryRequest {
        query: terms,
        page,
        per_page: per,
        sort: if searching && matches!(query.operation(), HistoryOperation::Commit) {
            Some("committer-date".into())
        } else if lists_issues_newest_first(&query) {
            Some("created".into())
        } else {
            query.sort().filter(|v| v != "best-match")
        },
        order: if searching && matches!(query.operation(), HistoryOperation::Commit) {
            Some("desc".into())
        } else if lists_issues_newest_first(&query) {
            Some(query.order().unwrap_or_else(|| "desc".into()))
        } else {
            query.order()
        },
    };
    let mut result = match query.operation() {
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
                        branch: query.branch().map(str::to_owned),
                        path: query.path().map(str::to_owned),
                        author: query.author().map(str::to_owned),
                        since,
                        until,
                        page,
                        per_page: per,
                    },
                    query.committer(),
                    context,
                )
                .await?;
            if listed.items.is_empty() && (query.since().is_some() || query.until().is_some()) {
                listed.warnings.push(
                    "since/until matched no commits (GitHub commit listing uses committer date and does not follow renames).".into(),
                );
            }
            listed
        }
        HistoryOperation::PullRequest if !searching => {
            let (o, r) = required_repo(&query)?;
            transport
                .list_pull_requests(
                    &PullListRequest {
                        owner: o.into(),
                        repo: r.into(),
                        state: query.state(),
                        head: query.head().map(str::to_owned),
                        base: query.base().map(str::to_owned),
                        sort: query.sort(),
                        order: query.order(),
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
    let mut value = match query.operation() {
        HistoryOperation::PullRequest => {
            let rows = result
                .items
                .iter()
                .cloned()
                .map(|item| {
                    if query.concise() == Some(true) {
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
                && let (Some(owner), Some(repo)) = (query.owner(), query.repo())
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
                    if query.concise() == Some(true) {
                        concise_row(&item)
                    } else {
                        map_issue(item)
                    }
                })
                .collect::<Vec<_>>();
            let mut v = json!({"type":"issues","owner":query.owner(),"repo":query.repo(),"issues":issues,"effectiveQuery":effective,"pagination":{"currentPage":current_page,"perPage":per,"hasMore":more,"nextPage":more.then_some(current_page+1)}});
            if let Some(total) = if result.listed {
                exact_list_total
            } else {
                Some(total)
            } {
                v["totalCount"] = json!(total);
            }
            if let Some(number) = result
                .items
                .first()
                .and_then(|x| x.get("number"))
                .and_then(Value::as_u64)
                && let (Some(owner), Some(repo)) = (query.owner(), query.repo())
            {
                v["next"]["readIssue"] = json!({"tool":"ghGetHistoryItem","query":{"operation":"issue","owner":owner,"repo":repo,"number":number,"content":{"body":true,"comments":{"discussion":true}}},"confidence":"low"});
            }
            v
        }
        HistoryOperation::Commit => {
            let mut v = json!({"type":"commits","owner":query.owner(),"repo":query.repo(),"scope":"defaultBranch","commits":result.items.into_iter().map(if query.keywords().is_empty(){map_commit_list}else{map_commit}).collect::<Vec<_>>(),"incompleteResults":result.incomplete_results,"pagination":{"page":page,"perPage":per,"hasMore":more}});
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
    if matches!(query.operation(), HistoryOperation::Commit) && query.keywords().is_empty() {
        let commits = value["commits"].as_array().cloned().unwrap_or_default();
        value = json!({"type":if query.path().as_ref().is_some_and(|p|!p.ends_with('/')){"file"}else{"repo"},"owner":query.owner(),"repo":query.repo(),"path":query.path(),"commits":commits});
        remove_null_fields(&mut value);
        if more {
            value["pagination"] =
                json!({"currentPage":page,"perPage":per,"hasMore":true,"nextPage":page+1});
        }
    }
    if matches!(query.operation(), HistoryOperation::Commit)
        && let Some(sha) = value
            .pointer("/commits/0/sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
        && let (Some(owner), Some(repo)) = (query.owner(), query.repo())
    {
        let mut read = json!({
            "operation":"commit","owner":owner,"repo":repo,"ref":sha,"includeDiff":true
        });
        if let Some(path) = query.path() {
            read["path"] = json!(path);
        }
        value["next"]["readCommit"] =
            json!({"tool":"ghGetHistoryItem","query":read,"confidence":"low"});
    }
    if !result.warnings.is_empty() {
        value["warnings"] = json!(result.warnings);
    }
    if matches!(query.operation(), HistoryOperation::Issue)
        && let Some(map) = value.as_object_mut()
        && !more
    {
        map.remove("pagination");
    }
    // List mode runs the REST endpoint, not the search terms: reporting them
    // as the effective query would misdescribe what executed.
    if result.listed
        && matches!(
            query.operation(),
            HistoryOperation::Issue | HistoryOperation::PullRequest
        )
        && let Some(map) = value.as_object_mut()
    {
        map.remove("effectiveQuery");
    }
    if matches!(query.operation(), HistoryOperation::Commit) && more {
        value["pagination"]["nextPage"] = json!(current_page + 1);
    }
    if more {
        let mut next = serde_json::to_value(&query).unwrap_or_default();
        remove_null_fields(&mut next);
        next["page"] = json!(current_page + 1);
        // The nextPage continuation validates against the input schema, whose
        // serialization makes the paginated defaulted fields required. Stamp the
        // effective page size so an unset pageSize still yields a valid,
        // directly-executable continuation.
        next["pageSize"] = json!(per);
        value["next"]["nextPage"] =
            json!({"tool":"ghSearchHistory","query":next,"confidence":"exact"});
    }
    remove_null_fields(&mut value);
    mark_empty(&mut value, more);
    if result.incomplete_results || (!result.listed && result.total_count > 1000) {
        value["isPartial"] = json!(true);
        if !more {
            value["terminalLimit"] = json!(true);
        }
        value["partialReasons"] = json!([if result.total_count > 1000 {
            "providerResultCap"
        } else {
            "providerIncompleteResults"
        }]);
        // Success rows keep warnings (hints are reserved for empty/error rows).
        if result.total_count > 1000 {
            match value.get_mut("warnings").and_then(Value::as_array_mut) {
                Some(warnings) => warnings.push(json!(CAP_PARTITION_HINT)),
                None => value["warnings"] = json!([CAP_PARTITION_HINT]),
            }
        }
    }
    Ok(value)
}
/// GitHub search stops at 1,000 results; the cap is not the end of history.
const CAP_PARTITION_HINT: &str = "GitHub search returns at most 1,000 results; partition by date (created/merged-at/closed ranges, or since/until for commits) or narrow keywords to reach the rest.";

/// A complete page with no rows is empty, so the shared fallback hint fires.
fn mark_empty(value: &mut Value, more: bool) {
    let rows = ["pullRequests", "issues", "commits"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array));
    if !more && rows.is_some_and(Vec::is_empty) {
        value["status"] = json!("empty");
    }
}

fn required_repo(q: &GhSearchHistoryQuery) -> Result<(&str, &str), ProviderError> {
    q.owner().zip(q.repo()).ok_or_else(|| {
        ProviderError::new(ProviderErrorKind::Validation, "owner and repo are required")
    })
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
    remove_null_fields(&mut row);
    row
}
fn concise_row(v: &Value) -> Value {
    json!(format!(
        "#{} {}",
        v.get("number").and_then(Value::as_u64).unwrap_or(0),
        v.get("title").and_then(Value::as_str).unwrap_or("")
    ))
}
/// A keyword-less issue listing keeps the REST list order (newest first)
/// instead of search's unordered best-match.
fn lists_issues_newest_first(q: &GhSearchHistoryQuery) -> bool {
    matches!(q.operation(), HistoryOperation::Issue)
        && q.keywords().is_empty()
        && q.sort().is_none_or(|sort| sort == "best-match")
}
/// Issue-style qualifiers only the search API understands.
fn needs_issue_search_qualifiers(q: &GhSearchHistoryQuery) -> bool {
    !q.keywords().is_empty()
        || q.author().is_some()
        || q.assignee().is_some()
        || !q.label().is_empty()
        || q.mentions().is_some()
        || q.commenter().is_some()
        || q.reactions().is_some()
        || q.comments().is_some()
        || q.created().is_some()
        || q.updated().is_some()
        || q.closed().is_some()
        || !q.match_kinds().is_empty()
        || matches!(q.sort().as_deref(), Some("comments" | "reactions"))
}
fn should_use_search_for_prs(q: &GhSearchHistoryQuery) -> bool {
    // The REST list endpoint needs owner+repo; anything broader is search.
    q.owner().is_none()
        || q.repo().is_none()
        || needs_issue_search_qualifiers(q)
        || q.draft().is_some()
        || q.reviewed_by().is_some()
        || q.review_requested().is_some()
        || q.checks().is_some()
        || q.review().is_some()
        || q.head().is_some()
        || q.base().is_some()
        || q.merged_at().is_some()
        || q.state().as_deref() == Some("merged")
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

/// Owner/repo become `repo:`/`user:` scopes and person fields become
/// `author:`-style qualifiers: reject anything that is not a GitHub name
/// before it can rewrite the search scope.
fn validate_history_scope(q: &GhSearchHistoryQuery) -> Result<(), ProviderError> {
    if let Some(owner) = q.owner() {
        validate_search_name("owner", owner, SearchName::Owner)?;
    }
    if let Some(repo) = q.repo() {
        validate_search_name("repo", repo, SearchName::Repository)?;
    }
    for (field, value) in [
        ("author", q.author()),
        ("committer", q.committer()),
        ("assignee", q.assignee()),
        ("mentions", q.mentions()),
        ("commenter", q.commenter()),
        ("reviewed-by", q.reviewed_by()),
        ("review-requested", q.review_requested()),
    ] {
        if let Some(value) = value {
            validate_search_name(field, value, SearchName::Person)?;
        }
    }
    Ok(())
}

/// Resolves `since`/`until`, collecting invalid-value warnings and rejecting
/// an inverted window (since after until) as a validation error.
fn resolve_commit_window(
    q: &GhSearchHistoryQuery,
    warnings: &mut Vec<String>,
) -> Result<(Option<String>, Option<String>), ProviderError> {
    let since = q.since().map(resolve_date_window);
    let until = q.until().map(resolve_date_window);
    if let (Some(since), Some(until)) = (&since, &until)
        && since.is_after(until)
    {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "since ({}) is after until ({}); swap them or widen the window",
                q.since().unwrap_or_default().trim(),
                q.until().unwrap_or_default().trim()
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
    validate_history_scope(q)?;
    let mut out = q
        .keywords()
        .into_iter()
        .map(quote_search_keyword)
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    // Range and enum qualifiers are single terms: whitespace inside a range
    // (`> 5`, `a .. b`) is dropped, anything that could open a new term is rejected.
    let push = |out: &mut Vec<String>, k: &str, v: Option<&str>| -> Result<(), ProviderError> {
        if let Some(v) = v {
            let range = v.trim_start().starts_with(['<', '>', '=']) || v.contains("..");
            let compact = if range {
                v.chars().filter(|c| !c.is_whitespace()).collect::<String>()
            } else {
                v.trim().to_owned()
            };
            validate_qualifier_value(k, &compact)?;
            out.push(format!("{k}:{compact}"));
        }
        Ok(())
    };
    match q.operation() {
        HistoryOperation::Commit => {
            let (o, r) = required_repo(q)?;
            out.push(format!("repo:{o}/{r}"));
            for (field, value) in [("author", q.author()), ("committer", q.committer())] {
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
            if !q.match_kinds().is_empty() {
                let fields: Vec<String> = q.match_kinds().iter().map(ToString::to_string).collect();
                push(&mut out, "in", Some(&fields.join(",")))?;
            }
            out.push(
                if matches!(q.operation(), HistoryOperation::PullRequest) {
                    "is:pr"
                } else {
                    "is:issue"
                }
                .into(),
            );
            match (q.owner(), q.repo(), q.operation()) {
                (Some(o), Some(r), _) => out.push(format!("repo:{o}/{r}")),
                // Pull-request search is cross-repo capable (contract: owner and
                // repo optional); issue search stays repository-scoped.
                (Some(o), None, HistoryOperation::PullRequest) => out.push(format!("user:{o}")),
                (None, _, HistoryOperation::PullRequest) => {}
                _ => {
                    required_repo(q)?;
                }
            }
            push(&mut out, "is", q.state().as_deref())?;
            if let Some(draft) = q.draft() {
                out.push(if draft { "is:draft" } else { "-is:draft" }.into());
            }
            for (k, v) in [
                ("author", q.author()),
                ("assignee", q.assignee()),
                ("mentions", q.mentions()),
                ("commenter", q.commenter()),
                ("reviewed-by", q.reviewed_by()),
                ("review-requested", q.review_requested()),
                ("head", q.head()),
                ("base", q.base()),
                ("created", q.created()),
                ("updated", q.updated()),
                ("merged", q.merged_at()),
                ("closed", q.closed()),
                ("comments", q.comments()),
                ("reactions", q.reactions()),
                ("review", q.review().as_deref()),
            ] {
                push(&mut out, k, v)?;
            }
            for label in q.label() {
                // A label is one quoted name; an interior quote or backslash
                // would close it early and splice the rest into the query.
                if label.contains(['"', '\\']) || label.trim().is_empty() {
                    return Err(ProviderError::new(
                        ProviderErrorKind::Validation,
                        format!("label: \"{label}\" cannot contain quotes or backslashes"),
                    ));
                }
                out.push(format!("label:\"{}\"", label.trim()));
            }
            if let Some(archived) = q.archived() {
                out.push(format!("archived:{archived}"));
            }
            push(&mut out, "status", q.checks().as_deref())?;
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
        let q: GhSearchHistoryQuery=serde_json::from_str(r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"],"state":"closed","match":["title"],"label":["bug"]}"#).expect("GitHub history search test data should be valid");
        assert_eq!(
            build_query(&q).expect("GitHub history search test data should be valid"),
            "x in:title is:issue repo:a/b is:closed label:\"bug\""
        );
        let archived: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"],"archived":true}"#,
        )
        .expect("valid");
        assert!(
            build_query(&archived)
                .expect("valid")
                .ends_with("archived:true")
        );
    }
    #[test]
    fn report_probes_cannot_rewrite_the_history_scope() {
        // A leading-quote keyword, an owner carrying operators,
        // and a label with an interior quote.
        let q = parse(
            r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"octocat","repo":"Hello-World","keywords":["\"hello\" NOT"]}"#,
        );
        assert_eq!(
            build_query(&q).expect("valid"),
            "\"hello NOT\" is:issue repo:octocat/Hello-World"
        );
        let q = parse(
            r#"{"operation":"pullRequest","goal":"test","reasoning":"test","owner":"a","keywords":["repo:evil/x","-y"]}"#,
        );
        assert_eq!(
            build_query(&q).expect("valid"),
            "\"repo:evil/x\" \"-y\" is:pr user:a"
        );
        for raw in [
            r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"octocat OR is:public","repo":"b","keywords":["hello"]}"#,
            r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"a","repo":"b:c","keywords":["hello"]}"#,
            r#"{"operation":"pullRequest","goal":"test","reasoning":"test","owner":"a","author":"x OR is:public"}"#,
            r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"a","repo":"b","label":["x\" OR is:public"]}"#,
            r#"{"operation":"issue","goal": "test", "reasoning":"test","owner":"a","repo":"b","created":"x OR is:public"}"#,
            r#"{"operation":"commit","goal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["x"],"committer":"a b"}"#,
        ] {
            let error = build_query(&parse(raw)).expect_err(raw);
            assert_eq!(error.kind, ProviderErrorKind::Validation, "{raw}");
        }
        let q = parse(
            r#"{"operation":"issue","goal": "test", "reasoning":"test","owner":"a","repo":"b","label":["good first issue"],"comments":"> 5","author":"dependabot[bot]"}"#,
        );
        let built = build_query(&q).expect("valid");
        assert!(built.contains("label:\"good first issue\""), "{built}");
        assert!(built.contains("comments:>5"), "{built}");
        assert!(built.contains("author:dependabot[bot]"), "{built}");
    }

    #[test]
    fn quotes_multiword_history_keywords() {
        let q: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix login"]}"#,
        )
        .expect("GitHub history search test data should be valid");
        assert!(
            build_query(&q)
                .expect("GitHub history search test data should be valid")
                .starts_with("\"fix login\"")
        );
        assert!(needs_issue_search_qualifiers(&q));
        let listed: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"issue","goal":"test","reasoning":"test","owner":"a","repo":"b"}"#,
        )
        .expect("GitHub history search test data should be valid");
        assert!(!needs_issue_search_qualifiers(&listed));
    }
    #[test]
    fn commit_search_uses_email_and_committer_date() {
        let q: GhSearchHistoryQuery = serde_json::from_str(
            r#"{"operation":"commit","goal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"author":"dev@example.com","since":"2026-01-01T00:00:00Z"}"#,
        )
        .expect("GitHub history search test data should be valid");
        let query = build_query(&q).expect("GitHub history search test data should be valid");
        assert!(query.contains("author-email:dev@example.com"));
        assert!(query.contains("committer-date:>=2026-01-01T00:00:00Z"));
    }
    #[test]
    fn rejects_unscoped_commit() {
        // Commit history is repository-scoped by the wire contract.
        assert!(
            serde_json::from_str::<GhSearchHistoryQuery>(
                r#"{"operation":"commit","goal":"test","reasoning":"test","owner":"a"}"#
            )
            .is_err()
        );
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

    #[test]
    fn empty_history_rows_are_marked_empty() {
        for key in ["pullRequests", "issues", "commits"] {
            let mut value = json!({ key: [] });
            mark_empty(&mut value, false);
            assert_eq!(value["status"], "empty", "{key}");
        }
        let mut more = json!({"commits": []});
        mark_empty(&mut more, true);
        assert!(more.get("status").is_none());
        let mut rows = json!({"issues": [{"number": 1}]});
        mark_empty(&mut rows, false);
        assert!(rows.get("status").is_none());
    }

    #[tokio::test]
    async fn cap_is_terminal_only_after_the_last_reachable_page() {
        use crate::providers::github::{
            CredentialSource, GitHubEndpoint, RetryPolicy, StaticCredentialResolver,
        };
        use std::{sync::Arc, time::Duration};
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        struct Passthrough;
        impl ContentScan for Passthrough {
            fn sanitize(
                &self,
                text: &str,
                _: &std::path::Path,
            ) -> Result<(String, Vec<String>), (String, String)> {
                Ok((text.to_owned(), vec![]))
            }
        }

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/issues"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 1001, "incomplete_results": false,
                "items": [{"number": 3, "title": "Fix", "state": "open", "user": {"login": "dev"}}]
            })))
            .mount(&server)
            .await;
        let transport = GitHubTransport::new(
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("url"))
                .expect("endpoint"),
            Arc::new(StaticCredentialResolver::new(
                "fixture",
                CredentialSource::Override,
            )),
            RetryPolicy {
                max_attempts: 1,
                ..Default::default()
            },
        )
        .expect("transport");
        for page in [1, 1000] {
            let query = serde_json::from_value(json!({"operation":"pullRequest","goal": "test", "reasoning":"test","keywords":["fix"],"pageSize":1,"page":page})).expect("query");
            let data = execute(
                &transport,
                &query,
                &RequestContext::with_timeout(Duration::from_secs(5), 1 << 20),
                &Passthrough,
            )
            .await
            .expect("history search");
            assert_eq!(data["isPartial"], true, "{data}");
            assert_eq!(
                data["partialReasons"],
                json!(["providerResultCap"]),
                "{data}"
            );
            assert_eq!(
                data["terminalLimit"].as_bool().unwrap_or(false),
                page == 1000,
                "{data}"
            );
            assert_eq!(data["next"]["nextPage"].is_object(), page < 1000, "{data}");
        }
    }

    /// D4: a plain issue listing pages `is:issue` search results, so every
    /// page holds up to pageSize real issues (the REST /issues list
    /// interleaves PRs) and page numbers advance by one. An empty or PR-only
    /// repository is a clean empty row.
    #[tokio::test]
    async fn plain_issue_listing_pages_issue_search_without_pr_gaps() {
        use crate::providers::github::{
            CredentialSource, GitHubEndpoint, RetryPolicy, StaticCredentialResolver,
        };
        use std::{sync::Arc, time::Duration};
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

        struct Passthrough;
        impl ContentScan for Passthrough {
            fn sanitize(
                &self,
                text: &str,
                _: &std::path::Path,
            ) -> Result<(String, Vec<String>), (String, String)> {
                Ok((text.to_owned(), vec![]))
            }
        }
        let issue = |n: u64| json!({"number": n, "title": format!("issue {n}"), "state": "open", "user": {"login": "dev"}});
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/issues"))
            .and(query_param("q", "is:issue repo:o/full"))
            .and(query_param("page", "2"))
            .and(query_param("per_page", "5"))
            .and(query_param("sort", "created"))
            .and(query_param("order", "desc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 11, "incomplete_results": false,
                "items": [issue(6), issue(7), issue(8), issue(9), issue(10)]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/issues"))
            .and(query_param("q", "is:issue repo:o/prs-only"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 0, "incomplete_results": false, "items": []
            })))
            .mount(&server)
            .await;
        let transport = GitHubTransport::new(
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("url"))
                .expect("endpoint"),
            Arc::new(StaticCredentialResolver::new(
                "fixture",
                CredentialSource::Override,
            )),
            RetryPolicy {
                max_attempts: 1,
                ..Default::default()
            },
        )
        .expect("transport");
        let run = |query: Value| {
            let transport = &transport;
            async move {
                execute(
                    transport,
                    &serde_json::from_value(query).expect("query"),
                    &RequestContext::with_timeout(Duration::from_secs(5), 1 << 20),
                    &Passthrough,
                )
                .await
                .expect("issue listing")
            }
        };
        let data = run(json!({"operation":"issue","goal":"g","reasoning":"r","owner":"o","repo":"full","pageSize":5,"page":2})).await;
        assert_eq!(data["issues"].as_array().map(Vec::len), Some(5), "{data}");
        assert_eq!(data["pagination"]["currentPage"], 2, "{data}");
        assert_eq!(data["pagination"]["nextPage"], 3, "{data}");
        assert_eq!(data["next"]["nextPage"]["query"]["page"], 3, "{data}");
        assert_eq!(data["totalCount"], 11, "{data}");
        assert!(data.get("skippedPullRequestPages").is_none(), "{data}");

        let empty = run(json!({"operation":"issue","goal":"g","reasoning":"r","owner":"o","repo":"prs-only","pageSize":5})).await;
        assert_eq!(empty["issues"], json!([]), "{empty}");
        assert_eq!(empty["status"], "empty", "{empty}");
        assert!(empty.get("pagination").is_none(), "{empty}");
        assert!(empty["next"].get("nextPage").is_none(), "{empty}");
    }

    fn parse(json: &str) -> GhSearchHistoryQuery {
        serde_json::from_str(json).expect("GitHub history search test data should be valid")
    }

    #[test]
    fn pull_request_search_allows_cross_repo_and_owner_scopes() {
        let both = build_query(&parse(
            r#"{"operation":"pullRequest","goal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"]}"#,
        ))
        .expect("scoped");
        assert!(both.contains("repo:a/b"), "{both}");
        let owner = build_query(&parse(
            r#"{"operation":"pullRequest","goal":"test","reasoning":"test","owner":"a","keywords":["x"]}"#,
        ))
        .expect("owner-scoped PR search");
        assert!(
            owner.contains("user:a") && !owner.contains("repo:"),
            "{owner}"
        );
        let global = build_query(&parse(
            r#"{"operation":"pullRequest","goal":"test","reasoning":"test","keywords":["x"]}"#,
        ))
        .expect("cross-repo PR search");
        assert!(
            !global.contains("repo:") && !global.contains("user:"),
            "{global}"
        );
        assert!(!global.contains("archived:"), "{global}");
        // Without a full repo scope the REST list endpoint is unusable, so the
        // PR path must route through search.
        assert!(should_use_search_for_prs(&parse(
            r#"{"operation":"pullRequest","goal":"test","reasoning":"test","owner":"a"}"#
        )));
        assert!(!should_use_search_for_prs(&parse(
            r#"{"operation":"pullRequest","goal":"test","reasoning":"test","owner":"a","repo":"b"}"#
        )));
        // Issues still require the repository scope.
        assert!(
            serde_json::from_str::<GhSearchHistoryQuery>(
                r#"{"operation":"issue","goal":"test","reasoning":"test","keywords":["x"]}"#
            )
            .is_err()
        );
    }

    #[test]
    fn commit_search_surfaces_invalid_date_warnings() {
        let q = parse(
            r#"{"operation":"commit","goal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"since":"yesterday-ish"}"#,
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
            r#"{"operation":"commit","goal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"since":"2026-05-01","until":"2026-01-01"}"#,
        );
        let error = build_query(&q).expect_err("since after until");
        assert_eq!(error.kind, ProviderErrorKind::Validation);
        assert!(error.message.contains("since"), "{}", error.message);
        let listed = parse(
            r#"{"operation":"commit","goal":"test","reasoning":"test","owner":"a","repo":"b","since":"2026-05-01","until":"2026-01-01"}"#,
        );
        assert!(build_query(&listed).is_err());
    }
}
