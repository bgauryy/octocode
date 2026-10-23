//! `operation: "issue"`: issue metadata, body window, and one page of
//! discussion comments.
use super::axis::Axis;
use super::promotion::promote_issue_continuations;
use super::remaining::Remainder;
use super::text::paginate_text;
use super::util::{str_at, string};
use super::{HistoryItemRequest, Section, Sections, fetch, validation};
use crate::response::result::ToolData;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::dates::{utc_date, utc_value};
use crate::tools::gh_shared::ensure_next;
use octocode_github::{
    GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::{Map, Value, json};

mod closing;
mod comments;

#[cfg(test)]
pub(super) use closing::CLOSING_REFERENCES_DOCUMENT;
use closing::{attach_fix_pr, closing_pull_requests};
use comments::{CommentRead, comment_batch, first_comment_batch, issue_comments};

/// A pull-request read that 404ed: when the number is an issue, say so with
/// a typed reason (the caller offers the issue read); otherwise keep the
/// original not-found.
pub(super) async fn name_issue_number(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    not_found: ProviderError,
) -> ProviderError {
    let Some(number) = gh_get_history_item_query::number(query).map(|number| number.to_string())
    else {
        return not_found;
    };
    let issue_path = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "issues",
        &number,
    ];
    match fetch(transport, &issue_path, &[], context).await {
        Ok((raw, _)) if raw.get("pull_request").is_none_or(Value::is_null) => {
            let mut error = ProviderError::new(
                ProviderErrorKind::NotFound,
                format!("#{number} is an issue, not a pull request"),
            )
            .with_reason(ProviderErrorReason::PullRequestIsIssue);
            error.status = not_found.status;
            error
        }
        _ => not_found,
    }
}

pub(super) async fn issue(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let number = gh_get_history_item_query::number(query)
        .ok_or_else(|| validation("number is required"))?
        .to_string();
    let issue_path = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "issues",
        &number,
    ];
    // The closing PRs load beside the issue on its first window, which
    // states their bound once; later windows list no `closedBy`.
    let first_window = gh_get_history_item_query::page(query).unwrap_or(1) <= 1
        && gh_get_history_item_query::char_offset(query).is_none_or(|offset| offset == 0);
    // The discussion's first comment batch depends only on the page asked
    // for, so it loads beside the issue too (wasted only when the number
    // names a pull request).
    let first_batch = query
        .sections()
        .has(Section::Comments)
        .then(|| first_comment_batch(query))
        .flatten();
    let (fetched, closed_by, prefetched) = tokio::join!(
        fetch(transport, &issue_path, &[], context),
        async {
            if first_window {
                closing_pull_requests(transport, query, context).await
            } else {
                None
            }
        },
        async {
            match first_batch {
                Some(batch) => Some(comment_batch(transport, &issue_path, batch, context).await),
                None => None,
            }
        }
    );
    let (raw, _) = fetched?;
    // The number names a pull request: not found as an issue, typed like a
    // pull-request read of an issue number ([`name_issue_number`]).
    if raw.get("pull_request").is_some_and(|v| !v.is_null()) {
        return Err(ProviderError::new(
            ProviderErrorKind::NotFound,
            format!("#{number} is a pull request, not an issue"),
        )
        .with_reason(ProviderErrorReason::IssueIsPullRequest));
    }
    // A summary (no section named) shows the body; a section read what it
    // names.
    let sections = query.sections();
    let want_body = sections.is_empty() || sections.has(Section::Body);
    let want_comments = sections.has(Section::Comments);
    // A comment-body hop (`offset` > 0 with the discussion) continues the
    // comment bodies of its first window. The issue body is not shown again
    // (its own `continueBody` reads it at its own offset), but its first
    // window still counts against the page, so the hop pages exactly the
    // comments that window showed.
    let comment_hop = want_comments
        && gh_get_history_item_query::char_offset(query).is_some_and(|offset| offset > 0);
    let show_body = want_body && !comment_hop;
    // A later comment page continues a read whose first page named the
    // issue; a section read's sections carry its evidence: either restates
    // only the number and state (the summary owns the rest).
    let later_page = gh_get_history_item_query::page(query).unwrap_or(1) > 1;
    let slim = later_page || gh_get_history_item_query::has_sections(query);
    let header = issue_header(&raw);
    let header_for = || {
        let mut row = header.clone();
        if slim && let Some(fields) = row.as_object_mut() {
            fields.retain(|key, _| key == "number" || key == "state");
        }
        row
    };
    let body_view = want_body.then(|| str_at(&raw, "/body").unwrap_or(""));
    let mut row = header_for();
    let mut pagination = Map::new();
    let mut remainder = Remainder::default();
    if let Some(body_view) = body_view.filter(|_| show_body) {
        let (body, page) = paginate_text(
            body_view,
            gh_get_history_item_query::char_offset(query),
            gh_get_history_item_query::char_length(query),
        );
        row["body"] = json!(body);
        if gh_get_history_item_query::char_offset(query).is_some()
            || gh_get_history_item_query::char_length(query).is_some()
            || page["hasMore"] == true
        {
            pagination.insert(Axis::Body.key().into(), page);
        }
    }
    if want_comments {
        // A comment-body hop counts the row its first window showed.
        let used = if comment_hop {
            let mut first = header_for();
            if let Some(body_view) = body_view {
                first["body"] = json!(
                    paginate_text(
                        body_view,
                        None,
                        gh_get_history_item_query::char_length(query)
                    )
                    .0
                );
            }
            octocode_engine::json_size::utf16_units(&first).unwrap_or(0)
        } else {
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0)
        };
        let at = CommentRead {
            issue_path: &issue_path,
            used,
            total: raw.get("comments").and_then(Value::as_u64),
        };
        let page = issue_comments(transport, query, context, &at, prefetched).await?;
        page.attach(&at, &mut row, &mut pagination, &mut remainder);
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    let closed = raw.get("state").and_then(Value::as_str) == Some("closed");
    if let Some(references) = closed_by.as_ref().filter(|refs| !refs.prs.is_empty()) {
        row["closedBy"] = Value::Array(references.prs.iter().map(|pr| pr.row.clone()).collect());
    }
    let mut out = json!({"issues":[row]});
    let remaining = promote_issue_continuations(&mut out, query, &remainder);
    if !want_comments && !later_page {
        attach_comments_read(&mut out, query, &raw);
    }
    let bounded = closed_by.as_ref().and_then(|references| {
        references
            .bounded_total
            .map(|total| (references.listed, total))
    });
    if let Some((listed, total)) = bounded {
        mark_bounded_closing(&mut out, listed, total);
    }
    if first_window {
        let prs = closed_by
            .as_ref()
            .map(|references| references.prs.as_slice());
        attach_fix_pr(&mut out, query, prs, closed, bounded.is_some());
    }
    Ok(ToolData {
        remaining,
        ..ToolData::from(out)
    })
}

/// The issue's identity and decision facts: number, title, state, author,
/// labels (when any), creation, the discussion's size, the last update of
/// an open issue, and the close date.
fn issue_header(raw: &Value) -> Value {
    json!({
        "number":raw["number"],"title":string(raw.get("title")),
        "state":str_at(raw,"/state").unwrap_or("open"),"author":str_at(raw,"/user/login").unwrap_or("unknown"),
        // Labels only when there are any.
        "labels":Some(raw.get("labels").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/name").map(str::to_owned)).collect::<Vec<_>>()).filter(|labels| !labels.is_empty()),
        "createdAt":utc_date(str_at(raw,"/created_at")),
        // The discussion's size, so the caller can choose to read it.
        "commentsCount":super::util::nonzero(raw.get("comments")),
        // Only an open issue's last update says whether it is still moving.
        "updatedAt":(str_at(raw,"/state").unwrap_or("open") == "open").then(|| utc_date(str_at(raw,"/updated_at"))),
        "closedAt":utc_value(raw.get("closed_at"))
    })
}

/// `next.readDiscussion`: an issue read without its discussion offers it
/// when GitHub counts any comment (the same issue, `sections:["comments"]`).
fn attach_comments_read(out: &mut Value, query: &HistoryItemRequest, raw: &Value) {
    if super::util::nonzero(raw.get("comments")).is_none() {
        return;
    }
    let mut nq = super::pr_menu::base_public_query(query, super::ItemOperation::Issue);
    for key in ["offset", "length", "page"] {
        super::pr_menu::remove_key(&mut nq, key);
    }
    Sections::of([Section::Comments]).set_on(&mut nq);
    ensure_next(out);
    out["next"]["readDiscussion"] = super::pr_menu::continuation(nq);
}

/// The closing-reference set ends at a bound: no cursor continues it, and the
/// fix it names is a candidate among the listed references only.
fn mark_bounded_closing(out: &mut Value, listed: usize, total: u64) {
    crate::response::paging::terminal_limit(
        out,
        "closingReferenceLimit",
        &[format!(
            "closedBy lists {listed} of {total} linked pull requests; the first is a candidate, not the complete fix set."
        )],
    );
}
