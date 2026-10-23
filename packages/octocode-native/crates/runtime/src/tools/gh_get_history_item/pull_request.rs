//! `operation: "pullRequest"`: concurrent collection loads (GraphQL first page
//! or REST windows), metadata row, and assembly of the shaped sections.
use super::filter::{FileFilter, FileScope};
use super::graphql::{
    GraphqlOutcome, GraphqlPr, graphql_complete_collection_eligible, graphql_pull_request,
};
use super::pr_finish::finish_pr;
use super::text::needle;
use super::util::str_at;
use super::{HistoryItemRequest, Section, validation};
use crate::response::result::ToolData;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use load::{load_inline, load_pr};
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Value, json};
use shape::shape_pr;

mod files;
mod head_sources;
mod load;
mod shape;

pub(super) use files::FileOutcome;
pub(super) use shape::ShapedPr;

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
