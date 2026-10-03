//! `operation: "issue"`: issue metadata, body window, and one page of
//! discussion comments.
use super::continuations::promote_issue_continuations;
use super::util::{
    array, content_flag, history_body_view, map_comments, merge, paginate_text, str_at, string,
    window_body,
};
use super::{HistoryItemRequest, default_page_size, fetch, validation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use serde_json::{Map, Value, json};

/// A pull-request read that 404ed: when the number is an issue, say so with
/// a typed reason (the caller offers the issue read); otherwise keep the
/// original not-found.
pub(super) async fn name_issue_number<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    not_found: ProviderError,
) -> ProviderError {
    let Some(number) = query.number().map(|number| number.to_string()) else {
        return not_found;
    };
    let issue_path = ["repos", query.owner(), query.repo(), "issues", &number];
    match fetch(transport, &issue_path, &[], context).await {
        Ok((raw, _)) if raw.get("pull_request").is_none_or(Value::is_null) => {
            let mut error = ProviderError::new(
                ProviderErrorKind::NotFound,
                format!(
                    "#{number} is an issue, not a pull request; read it with operation:\"issue\"."
                ),
            )
            .with_reason(ProviderErrorReason::PullRequestIsIssue);
            error.status = not_found.status;
            error
        }
        _ => not_found,
    }
}

pub(super) async fn issue<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    let number = query
        .number()
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let issue_path = ["repos", query.owner(), query.repo(), "issues", &number];
    // The closing PRs load beside the issue on its first window; later
    // windows ask only for their count so the bounded set stays disclosed.
    let first_window = query.comment_page().unwrap_or(1) <= 1
        && query.char_offset().is_none_or(|offset| offset == 0);
    let (fetched, closed_by) = tokio::join!(fetch(transport, &issue_path, &[], context), async {
        if first_window {
            closing_pull_requests(transport, query, context).await
        } else {
            closing_reference_count(transport, query, context).await
        }
    });
    let (raw, _) = fetched?;
    if raw.get("pull_request").is_some_and(|v| !v.is_null()) {
        return Err(validation(&format!(
            "Issue #{number} is a pull request; use ghGetHistoryItem operation:\"pullRequest\" with number:{number}."
        ))
        .with_reason(ProviderErrorReason::IssueIsPullRequest));
    }
    let content_value = query.content_value();
    let content = content_value.as_ref().and_then(Value::as_object);
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
        "createdAt":string(raw.get("created_at")),
        // Only an open issue's last update says whether it is still moving.
        "updatedAt":(str_at(&raw,"/state").unwrap_or("open") == "open").then(|| string(raw.get("updated_at"))),
        "closedAt":raw.get("closed_at").filter(|v|!v.is_null())
    });
    let mut pagination = Map::new();
    if want_body {
        let body_view = history_body_view(str_at(&raw, "/body").unwrap_or(""), query);
        let (body, page) = paginate_text(&body_view, query.char_offset(), query.char_length());
        row["body"] = json!(body);
        if query.char_offset().is_some() || query.char_length().is_some() || page["hasMore"] == true
        {
            pagination.insert("body".into(), page);
        }
    }
    if want_comments {
        let page_no = query.comment_page().unwrap_or(1);
        let per = query.page_size().unwrap_or_else(default_page_size);
        let (raw_comments, more) = if page_no == 0 {
            (json!([]), false)
        } else {
            fetch(
                transport,
                &[issue_path.as_slice(), &["comments"]].concat(),
                &[("per_page", per.to_string()), ("page", page_no.to_string())],
                context,
            )
            .await?
        };
        let mut shaped = Vec::new();
        let mut body_page = None;
        for comment in map_comments(array(raw_comments), "discussion", include_bots) {
            let (body, page) = window_body(
                str_at(&comment, "/body").unwrap_or(""),
                query.char_offset(),
                query,
                &mut body_page,
                // Issue text is never minified.
                &mut false,
            );
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
        // `raw.comments` is the issue's real comment count; the page count is
        // `returnedComments` (bots may be hidden from it).
        pagination.insert("comments".into(),json!({"currentPage":page_no,"itemsPerPage":per,"totalComments":raw.get("comments").and_then(Value::as_u64),"returnedComments":row["comments"].as_array().map_or(0,Vec::len),"hasMore":more,"nextCommentPage":more.then_some(page_no+1)}));
        if let Some(page) = body_page {
            pagination.insert("commentBody".into(), page);
        }
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    let closed = raw.get("state").and_then(Value::as_str) == Some("closed");
    if let Some(references) = closed_by.as_ref().filter(|refs| !refs.prs.is_empty()) {
        row["closedBy"] = Value::Array(references.prs.iter().map(|pr| pr.row.clone()).collect());
    }
    let mut out = json!({"type":"issues","owner":query.owner(),"repo":query.repo(),"issues":[row],"totalCount":1});
    promote_issue_continuations(&mut out, query);
    let bounded = closed_by.as_ref().and_then(|references| {
        references
            .bounded_total
            .map(|total| (references.listed, total))
    });
    if let Some((listed, total)) = bounded {
        // No cursor continues the set: it ends here, and the fix it names
        // is a candidate among the listed references only.
        out["isPartial"] = json!(true);
        out["terminalLimit"] = json!(true);
        let mut reasons = out["partialReasons"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        reasons.push(json!("closingReferenceLimit"));
        out["partialReasons"] = Value::Array(reasons);
        out["warnings"] = json!([if first_window {
            format!(
                "closedBy lists {listed} of {total} linked pull requests; readFixPr is a candidate, not the complete fix set."
            )
        } else {
            format!(
                "The first window's closedBy lists {listed} of {total} linked pull requests; the rest are not listed."
            )
        }]);
    }
    if first_window {
        let prs = closed_by
            .as_ref()
            .map(|references| references.prs.as_slice());
        attach_fix_pr(&mut out, query, prs, closed, bounded.is_some());
    }
    Ok(out)
}

/// Closing references one issue read lists. Past it the set is bounded and
/// the response says so.
const MAX_CLOSING_REFERENCES: usize = 25;

/// The closing-reference lookup; `first` is [`MAX_CLOSING_REFERENCES`].
pub(super) const CLOSING_REFERENCES_DOCUMENT: &str = "query($owner:String!,$repo:String!,$number:Int!,$first:Int!){ repository(owner:$owner,name:$repo){ issue(number:$number){ closedByPullRequestsReferences(first:$first,includeClosedPrs:true){ totalCount pageInfo{ hasNextPage } nodes{ number state mergedAt additions deletions changedFiles } } } } }";

/// The closing-reference count alone, for windows past the first.
pub(super) const CLOSING_REFERENCE_COUNT_DOCUMENT: &str = "query($owner:String!,$repo:String!,$number:Int!){ repository(owner:$owner,name:$repo){ issue(number:$number){ closedByPullRequestsReferences(first:1,includeClosedPrs:true){ totalCount } } } }";

/// One linked pull request: its public `closedBy` row and whether its whole
/// diff fits one patch read.
struct ClosingPr {
    row: Value,
    small: bool,
}

/// The linked pull requests one read returned, merged first, how many the
/// first window lists, and GitHub's total when more exist than that.
struct ClosingReferences {
    prs: Vec<ClosingPr>,
    listed: usize,
    bounded_total: Option<u64>,
}

/// The pull requests whose merge closes (or closed) the issue, merged first:
/// `{number, state, mergedAt?}`. `None` when GraphQL is unavailable or
/// failed (the caller falls back to search); empty when none are linked.
async fn closing_pull_requests<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingReferences> {
    if !transport.graphql_enabled || !transport.graphql_available(context).await {
        return None;
    }
    let variables = json!({
        "owner": query.owner(),
        "repo": query.repo(),
        "number": query.number()?,
        "first": MAX_CLOSING_REFERENCES,
    });
    let page = transport
        .execute_graphql(CLOSING_REFERENCES_DOCUMENT, variables, context)
        .await
        .ok()?;
    let references = page
        .data
        .as_ref()?
        .pointer("/repository/issue/closedByPullRequestsReferences")?;
    let more = references
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        == Some(true);
    let prs = map_closing_pull_requests(references.get("nodes")?.as_array()?);
    let listed = u64::try_from(prs.len()).unwrap_or(u64::MAX);
    Some(ClosingReferences {
        bounded_total: more.then(|| {
            references
                .get("totalCount")
                .and_then(Value::as_u64)
                .map_or(listed, |total| total.max(listed))
        }),
        listed: prs.len(),
        prs,
    })
}

/// The closing-reference total for a window past the first: no rows, only
/// whether the set the first window lists is bounded.
async fn closing_reference_count<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingReferences> {
    if !transport.graphql_enabled || !transport.graphql_available(context).await {
        return None;
    }
    let variables = json!({
        "owner": query.owner(),
        "repo": query.repo(),
        "number": query.number()?,
    });
    let page = transport
        .execute_graphql(CLOSING_REFERENCE_COUNT_DOCUMENT, variables, context)
        .await
        .ok()?;
    let total = page
        .data
        .as_ref()?
        .pointer("/repository/issue/closedByPullRequestsReferences/totalCount")?
        .as_u64()?;
    let limit = u64::try_from(MAX_CLOSING_REFERENCES).unwrap_or(u64::MAX);
    Some(ClosingReferences {
        prs: Vec::new(),
        listed: MAX_CLOSING_REFERENCES.min(usize::try_from(total).unwrap_or(usize::MAX)),
        bounded_total: (total > limit).then_some(total),
    })
}

fn map_closing_pull_requests(nodes: &[Value]) -> Vec<ClosingPr> {
    let mut prs = nodes
        .iter()
        .filter_map(|node| {
            let number = node.get("number").and_then(Value::as_u64)?;
            let mut row = json!({
                "number": number,
                "state": str_at(node, "/state").unwrap_or("closed").to_ascii_lowercase(),
                "mergedAt": node.get("mergedAt").filter(|v| !v.is_null()),
            });
            remove_nulls(&mut row);
            let count = |key: &str| node.get(key).and_then(Value::as_u64);
            let small = super::continuations::is_small_pr(
                count("additions"),
                count("deletions"),
                count("changedFiles"),
            );
            Some(ClosingPr { row, small })
        })
        .collect::<Vec<_>>();
    // Stable: merged first, otherwise GitHub's order.
    prs.sort_by_key(|pr| pr.row.get("mergedAt").is_none());
    prs
}

/// `next.readFixPr` reads the first linked (merged-first) pull request;
/// with no link information a closed issue offers the keyword search hop.
fn attach_fix_pr(
    out: &mut Value,
    query: &HistoryItemRequest,
    closed_by: Option<&[ClosingPr]>,
    closed: bool,
    bounded: bool,
) {
    let confidence = if bounded { "medium" } else { "high" };
    let next = match closed_by.and_then(<[ClosingPr]>::first) {
        // A small fix reads whole in one call: its patches are the evidence
        // (`closedBy` already links it to this issue). A larger one names
        // its files, beside its description, so the review can pick patches.
        // Files the caller's goal names: their patches are the evidence, and
        // the filter rides the copied query, so the narrowing is visible.
        Some(pr) => {
            let named = goal_file_globs(query);
            let mut read = json!({"tool":ToolId::GhGetHistoryItem.as_str(),"confidence":confidence,"query":{
                "operation":"pullRequest","owner":query.owner(),"repo":query.repo(),
                "number":pr.row["number"],
                "include": if pr.small || !named.is_empty() { json!(["patches"]) } else { json!(["body", "files"]) }}});
            if !named.is_empty() {
                read["query"]["fileFilter"] = json!({"paths": named});
            }
            ("readFixPr", read)
        }
        None if closed && closed_by.is_none() => (
            "findFixPr",
            json!({"tool":ToolId::GhSearchHistory.as_str(),"confidence":"medium","query":{
                "operation":"pullRequest","owner":query.owner(),"repo":query.repo(),
                "keywords":[query.number().unwrap_or_default().to_string()]}}),
        ),
        None => return,
    };
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"][next.0] = next.1;
}

/// Most goal-named files one fix-PR read narrows to.
const MAX_GOAL_FILES: usize = 5;

/// `fileFilter.paths` globs for file names the query's goal mentions
/// (`merge.go`, `pkg/cmd/merge.go`): a word counts only when its name has a
/// known code, config, doc, or lock file type, so dotted identifiers
/// (`res.redirect`) and versions never narrow a read.
fn goal_file_globs(query: &HistoryItemRequest) -> Vec<String> {
    use crate::content::classify_file_type;
    let goal = serde_json::to_value(&query.query)
        .ok()
        .and_then(|value| {
            value
                .get("mainGoal")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let mut globs = Vec::new();
    for word in goal.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))) {
        let path = word.trim_matches(|c| c == '.' || c == '/');
        let name = path.rsplit('/').next().unwrap_or(path);
        let has_stem = name
            .rsplit_once('.')
            .is_some_and(|(stem, _)| stem.chars().any(char::is_alphanumeric));
        if !has_stem || classify_file_type(name).is_none() {
            continue;
        }
        let glob = format!("**/{path}");
        if !globs.contains(&glob) {
            globs.push(glob);
        }
        if globs.len() == MAX_GOAL_FILES {
            break;
        }
    }
    globs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small merged fix is read with its patches in the same call; a large
    /// one (or one GitHub sized nothing for) reads its file inventory first.
    #[test]
    fn read_fix_pr_carries_the_patches_of_a_small_fix() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("issue query");
        let prs = map_closing_pull_requests(&[
            json!({"number":7,"state":"CLOSED","mergedAt":null,"additions":1,"deletions":0,"changedFiles":1}),
            json!({"number":8,"state":"MERGED","mergedAt":"2026-09-11T15:55:40Z",
                "additions":63,"deletions":4,"changedFiles":3}),
        ]);
        assert_eq!(
            prs.iter().map(|pr| pr.row.clone()).collect::<Vec<_>>(),
            [
                json!({"number":8,"state":"merged","mergedAt":"2026-09-11T15:55:40Z"}),
                json!({"number":7,"state":"closed"}),
            ]
        );
        let mut out = json!({});
        attach_fix_pr(&mut out, &query, Some(&prs), true, false);
        assert_eq!(out["next"]["readFixPr"]["query"]["number"], 8);
        assert_eq!(
            out["next"]["readFixPr"]["query"]["include"],
            json!(["patches"])
        );
        for node in [
            json!({"number":9,"state":"MERGED","mergedAt":"x","additions":900,"deletions":4,"changedFiles":3}),
            json!({"number":9,"state":"MERGED","mergedAt":"x"}),
        ] {
            let mut out = json!({});
            attach_fix_pr(
                &mut out,
                &query,
                Some(&map_closing_pull_requests(&[node])),
                true,
                false,
            );
            assert_eq!(
                out["next"]["readFixPr"]["query"]["include"],
                json!(["body", "files"])
            );
        }
        assert!(
            out_for(&query, &prs)["next"]["readFixPr"]["query"]
                .get("fileFilter")
                .is_none()
        );
    }

    fn out_for(query: &HistoryItemRequest, prs: &[ClosingPr]) -> Value {
        let mut out = json!({});
        attach_fix_pr(&mut out, query, Some(prs), true, false);
        out
    }

    /// A goal that names changed files reads only those files' patches of
    /// the fix; dotted words that are not file names (`res.redirect`,
    /// versions) never narrow it.
    #[test]
    fn read_fix_pr_narrows_to_files_the_goal_names() {
        let issue = |goal: &str| {
            HistoryItemRequest::from_row(json!({
                "operation":"issue","mainGoal":goal,"reasoning":"r","owner":"o","repo":"r","number":1
            }))
            .expect("issue query")
        };
        let large = map_closing_pull_requests(&[
            json!({"number":9,"state":"MERGED","mergedAt":"x","additions":900,"deletions":4,"changedFiles":40}),
        ]);
        let out = out_for(
            &issue("Which PR fixed cli/cli#14404 and what did it change in merge.go?"),
            &large,
        );
        let read = &out["next"]["readFixPr"]["query"];
        assert_eq!(
            read["fileFilter"],
            json!({"paths":["**/merge.go"]}),
            "{read}"
        );
        assert_eq!(read["include"], json!(["patches"]), "{read}");
        let out = out_for(
            &issue("Check pkg/cmd/pr/merge/merge.go and README.md (v2.31.0)."),
            &large,
        );
        assert_eq!(
            out["next"]["readFixPr"]["query"]["fileFilter"]["paths"],
            json!(["**/pkg/cmd/pr/merge/merge.go", "**/README.md"])
        );
        for goal in [
            "What does res.redirect default to in express@4.21.2?",
            "Which PR fixed this issue?",
        ] {
            let read = &out_for(&issue(goal), &large)["next"]["readFixPr"]["query"];
            assert!(read.get("fileFilter").is_none(), "{goal}: {read}");
            assert_eq!(read["include"], json!(["body", "files"]), "{goal}");
        }
    }
}
