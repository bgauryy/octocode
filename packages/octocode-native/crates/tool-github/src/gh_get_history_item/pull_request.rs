//! `operation: "pullRequest"`: concurrent collection loads (GraphQL first page
//! or REST windows), metadata row, and assembly of the shaped sections.
use super::filter::{FileFilter, FileScope};
use super::graphql::{GraphqlPr, graphql_complete_collection_eligible, graphql_pull_request};
use super::pr_finish::finish_pr;
use super::text::needle;
use super::util::str_at;
use super::{HistoryItemRequest, validation};
use crate::gh_get_history_item::Source;
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::sections::Section;
use load::{InlineWindow, load_inline, load_pr};
use octocode_github::{ProviderError, RequestContext};
use octocode_runtime_base::response::result::ToolData;
use serde_json::{Value, json};
use shape::shape_pr;

mod files;
mod head_sources;
mod load;
mod shape;

pub(super) use files::FileOutcome;
pub(super) use shape::ShapedPr;

pub(super) async fn pull_request(
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let wants = query.sections();
    let plan = PrPlan::new(query)?;
    // Review comments are REST-only: when GraphQL serves the rest of the
    // read, their first window loads beside it instead of after its answer.
    let (graphql, inline) =
        if graphql_complete_collection_eligible(query) && wants.has(Section::ReviewComments) {
            let (fast, inline) = tokio::try_join!(
                fast_path(source, query, context),
                load_inline(source, query, context, &plan, None),
            )?;
            (fast, InlineWindow::Prefetched(inline))
        } else {
            (
                fast_path(source, query, context).await?,
                InlineWindow::Pending,
            )
        };
    // A summary states GitHub's whole comment count, as ghSearchHistory
    // does: read beside the REST pull request, not after it.
    let summary = wants.is_empty();
    let (mut loads, whole_comments) = tokio::try_join!(
        load_pr(
            source,
            query,
            context,
            &wants,
            &plan,
            graphql.as_ref(),
            inline,
        ),
        async {
            if summary {
                whole_comment_count(source, query, context).await
            } else {
                Ok(None)
            }
        },
    )?;
    if let Some(count) = whole_comments {
        loads.raw["comments"] = json!(count);
    }
    let head = str_at(&loads.raw, "/head/sha").map(str::to_owned);
    // A pin names the head by its SHA or an abbreviation of it.
    if let Some(pinned) = gh_get_history_item_query::head_sha(&query.query)
        && !head.as_deref().is_some_and(|head| head.starts_with(pinned))
    {
        // The head moved since the first page: nothing shown stays valid.
        return Ok(ToolData::from(
            octocode_runtime_base::response::result::stale_snapshot(
                octocode_runtime_base::response::result::restart_query(
                    octocode_runtime_base::id::ToolId::GhGetHistoryItem,
                    &octocode_runtime_base::response::result::query_row(&query.query, &[]),
                ),
            ),
        ));
    }
    let shaped = shape_pr(source, query, context, &wants, &plan, loads).await?;
    let mut data = finish_pr(query, &wants, &plan, shaped);
    if summary && whole_comments.is_none() {
        octocode_runtime_base::response::paging::push_warnings(
            &mut data.data,
            [octocode_tools_shared::gh_shared::history::CONVERSATION_ONLY.to_owned()],
        );
    }
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
            if let Some(Value::Object(next)) = object.get_mut("calls") {
                let tool = octocode_runtime_base::id::ToolId::GhGetHistoryItem;
                for page in next.values_mut() {
                    if page["kind"] != "continue" || page["tool"] != tool.as_str() {
                        continue;
                    }
                    let rows = page
                        .pointer_mut("/input/queries")
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
                if key != "calls" && key != "hints" {
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

/// GitHub's whole comment count of the pull request (`totalCommentsCount`:
/// conversation, inline review comments and review bodies), the count
/// ghSearchHistory states and sorts by. `None` when GraphQL is unavailable
/// or does not answer it; a cancelled or expired request fails the read.
async fn whole_comment_count(
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Option<u64>, ProviderError> {
    let Some((document, variables)) = super::graphql::whole_count_read(query) else {
        return Ok(None);
    };
    // GitHub answers GraphQL to a credential only.
    if context.resolved_credential().is_none() || !source.graphql_available(context) {
        return Ok(None);
    }
    match source
        .execute_pull_request_graphql(&document, variables, context)
        .await
    {
        Ok(page) => Ok(page
            .data
            .as_ref()
            .and_then(|data| data.pointer("/repository/pullRequest/totalCommentsCount"))
            .and_then(Value::as_u64)),
        Err(error) if error.is_cancelled_or_timed_out() => Err(error),
        Err(_) => Ok(None),
    }
}

/// The GraphQL fast path. REST serves whatever GraphQL could not, a failed
/// fast path included; a cancelled or expired request stops here.
async fn fast_path(
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Option<GraphqlPr>, ProviderError> {
    if !graphql_complete_collection_eligible(query) {
        return Ok(None);
    }
    match graphql_pull_request(source, query, context).await {
        Err(error) if error.is_cancelled_or_timed_out() => Err(error),
        // REST serves the whole read and answers any failure itself.
        Err(_) => Ok(None),
        served => served,
    }
}

/// What a pull-request read selects: the literal and the file scope.
pub(super) struct PrPlan {
    number: String,
    pub(super) needle: Option<std::sync::Arc<super::matcher::Matcher>>,
    pub(super) scope: Option<FileScope>,
    /// `bots:false`: the discussion keeps the entries a person wrote.
    pub(super) humans: bool,
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
            humans: !gh_get_history_item_query::bots(query),
        })
    }

    /// Whether the discussion sections keep only some entries (a
    /// `matchString`, `bots:false`): each is then scanned from its first
    /// batch, and shaping drops the rest before paging.
    pub(super) fn filters_discussion(&self) -> bool {
        self.needle.is_some() || self.humans
    }

    /// Whether the discussion keeps `entry` ([`super::body_match::keeps`]).
    pub(super) fn keeps(&self, entry: &Value) -> bool {
        super::body_match::keeps(self.needle.as_deref(), self.humans, entry)
    }
    fn file_filter(&self) -> FileFilter<'_> {
        FileFilter {
            needle: self.needle.as_deref(),
            scope: self.scope.as_ref(),
        }
    }
}
