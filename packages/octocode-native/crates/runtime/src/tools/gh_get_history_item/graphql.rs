//! GraphQL pull-request fast path.
//!
//! One GraphQL request can serve a first page of several PR collections at
//! once. This module decides eligibility, runs that request, reads on from a
//! connection's end cursor until the first page is covered, and projects the
//! GraphQL shape into the REST-shaped values the rest of the tool consumes,
//! with per-collection completeness state.

use super::window::{MAX_COLLECTION_BATCHES, MAX_FILE_BATCHES, MAX_PR_COMMIT_BATCHES};
use super::{HistoryItemRequest, ItemOperation};
use crate::tools::gh_get_history_item::Source;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::sections::Section;
use octocode_github::{GraphQlError, ProviderError, RequestContext};
use serde_json::{Value, json};

mod document;
mod errors;
mod projection;
#[cfg(test)]
mod tests;

use document::{
    COMMITS, CONNECTION_PAGE, Connection, DISCUSSION, FILES, LABELS, REVIEWS, pull_request_document,
};
use errors::{connection_warning, describe_errors, error_field, graphql_failure};
pub(super) use projection::{
    GraphqlCollection, map_graphql_comments, map_graphql_commits, map_graphql_files,
    map_graphql_reviews,
};
use projection::{graphql_collection_state, map_graphql_pr_metadata};

/// The first-page PR served by GraphQL: REST-shaped metadata, the raw
/// GraphQL node, and per-collection completeness.
pub(super) struct GraphqlPr {
    pub(super) raw: Value,
    pub(super) source: Value,
    pub(super) files: GraphqlCollection,
    pub(super) discussion: GraphqlCollection,
    pub(super) reviews: GraphqlCollection,
    pub(super) commits: GraphqlCollection,
    /// One warning per connection GraphQL answered with `errors[]`: REST
    /// read it instead, and the warning names each error's path and text.
    pub(super) warnings: Vec<String>,
}

/// What GraphQL serves for one read: the wanted collections that no filter
/// narrows. A filtered collection needs every provider batch, which REST
/// reads at once from the PR's counts with full rows (a rename's previous
/// path, which an `include` scope matches); GraphQL would page it one
/// cursor at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct GraphqlWants {
    pub(super) body: bool,
    pub(super) files: bool,
    pub(super) discussion: bool,
    pub(super) reviews: bool,
    pub(super) commits: bool,
}

impl GraphqlWants {
    pub(super) fn of(query: &HistoryItemRequest) -> Self {
        let wants = query.sections();
        let unmatched = super::text::needle(query).is_none();
        Self {
            body: wants.has(Section::Body),
            files: wants.lists_files()
                && unmatched
                && !gh_get_history_item_query::has_file_filter(&query.query),
            discussion: wants.has(Section::Comments) && unmatched,
            reviews: wants.has(Section::Reviews) && unmatched,
            commits: wants.lists_commits(),
        }
    }

    fn count(self) -> usize {
        [
            self.body,
            self.files,
            self.discussion,
            self.reviews,
            self.commits,
        ]
        .into_iter()
        .filter(|wanted| *wanted)
        .count()
    }
}

/// GraphQL pays off only for a first page of at least two collections it
/// serves and no patches (GraphQL returns no patch text).
pub(super) fn graphql_complete_collection_eligible(query: &HistoryItemRequest) -> bool {
    if gh_get_history_item_query::operation(query) != ItemOperation::PullRequest {
        return false;
    }
    if gh_get_history_item_query::page(query).unwrap_or(1) > 1
        || query.sections().has(Section::Patches)
    {
        return false;
    }
    GraphqlWants::of(query).count() >= 2
}

/// What the GraphQL fast path produced for one pull-request read.
pub(super) enum GraphqlOutcome {
    /// GraphQL is disabled or unavailable for this endpoint: REST serves the
    /// read as the configured path, with nothing to report.
    Unavailable,
    /// GraphQL answered without a usable pull request; REST serves the read
    /// and the reason stays observable.
    Failed(String),
    Served(Box<GraphqlPr>),
}

/// Fetch the PR and its wanted collections in one GraphQL request, then
/// read on from each connection's end cursor until its first page is held.
pub(super) async fn graphql_pull_request(
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<GraphqlOutcome, ProviderError> {
    if !source.graphql_available(context) {
        return Ok(GraphqlOutcome::Unavailable);
    }
    let Some(number) = gh_get_history_item_query::number(query) else {
        return Ok(GraphqlOutcome::Unavailable);
    };
    let wants = GraphqlWants::of(query);
    let (document, bound) = pull_request_document(&wants);
    let mut variables = json!({
        "owner": gh_get_history_item_query::owner(query),
        "repo": gh_get_history_item_query::repo(query),
        "number": number
    });
    for (variable, first) in bound {
        variables[variable] = json!(first);
    }
    let mut page = source
        .execute_pull_request_graphql(&document, variables.clone(), context)
        .await?;
    let mut pr = page
        .data
        .as_mut()
        .and_then(|value| value.pointer_mut("/repository/pullRequest"))
        .map(Value::take)
        .unwrap_or(Value::Null);
    if pr.is_null() {
        return Ok(GraphqlOutcome::Failed(graphql_failure(&page.errors)));
    }
    let collection_page = gh_get_history_item_query::collection_page_size(query);
    let file_page = super::inventory::file_page_size(query, false);
    // Each wanted connection, the nodes its first page needs, and the
    // pages REST would read at most.
    let plans = [
        (&FILES, wants.files, file_page, MAX_FILE_BATCHES),
        (
            &DISCUSSION,
            wants.discussion,
            collection_page,
            MAX_COLLECTION_BATCHES,
        ),
        (
            &REVIEWS,
            wants.reviews,
            collection_page,
            MAX_COLLECTION_BATCHES,
        ),
        (
            &COMMITS,
            wants.commits,
            collection_page,
            MAX_PR_COMMIT_BATCHES,
        ),
    ];
    // An error beside the data nulls what its path names. Inside a wanted
    // connection it leaves only that connection to REST; anywhere else (a
    // header field, the labels, the pull request itself) no GraphQL field
    // can be trusted, and REST serves the whole read.
    let in_connection = |error: &GraphQlError| {
        error_field(error).is_some_and(|field| {
            plans
                .iter()
                .any(|(connection, wanted, ..)| *wanted && connection.key == field)
        })
    };
    if !page.errors.iter().all(in_connection) {
        return Ok(GraphqlOutcome::Failed(graphql_failure(&page.errors)));
    }
    // Every label, page by page within the request's deadline: a label the
    // read could not reach stays disclosed (`labelsClipped`). A labels page
    // answered with errors leaves the header to REST.
    let mut label_errors = Vec::new();
    read_on(
        source,
        context,
        &variables,
        &mut pr,
        &LABELS,
        (usize::MAX, usize::MAX),
        &mut label_errors,
    )
    .await?;
    if !label_errors.is_empty() {
        return Ok(GraphqlOutcome::Failed(label_errors.join("; ")));
    }
    let mut states = [GraphqlCollection::Unused; 4];
    let mut warnings = Vec::new();
    for (slot, (connection, wanted, need, max_pages)) in states.iter_mut().zip(plans) {
        if !wanted {
            continue;
        }
        let errors = page
            .errors
            .iter()
            .filter(|error| error_field(error) == Some(connection.key))
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            // Its nodes are untrusted: REST reads the connection whole.
            *slot = GraphqlCollection::Incomplete;
            warnings.push(connection_warning(connection, &describe_errors(errors)));
            continue;
        }
        let mut page_errors = Vec::new();
        *slot = read_on(
            source,
            context,
            &variables,
            &mut pr,
            connection,
            (need, max_pages),
            &mut page_errors,
        )
        .await?;
        if !page_errors.is_empty() {
            warnings.push(connection_warning(connection, &page_errors.join("; ")));
        }
    }
    // Redacted whole, before any row is mapped, filtered or windowed.
    source.redact_bodies(&mut pr)?;
    let [files, discussion, reviews, commits] = states;
    Ok(GraphqlOutcome::Served(Box::new(GraphqlPr {
        files,
        discussion,
        reviews,
        commits,
        warnings,
        raw: map_graphql_pr_metadata(&pr),
        source: pr,
    })))
}

/// Read `connection` on from its end cursor, appending every page's nodes
/// to `pr`, until it ends, `need` nodes are held, or `max_pages` pages were
/// read (`(need, max_pages)`); then its state. A connection the PR counts is
/// never read on: REST reads the rest in one wave ([`Connection::total`]). A
/// continuation that fails (other than by cancellation or timeout) leaves
/// the connection to REST; one answered with `errors[]` is not appended,
/// and `errors` receives its described errors.
async fn read_on(
    source: &Source<'_>,
    context: &RequestContext,
    variables: &Value,
    pr: &mut Value,
    connection: &Connection,
    (need, max_pages): (usize, usize),
    errors: &mut Vec<String>,
) -> Result<GraphqlCollection, ProviderError> {
    let mut pages = 1;
    loop {
        let state = graphql_collection_state(pr, connection.key);
        let held = &pr[connection.key];
        if state == GraphqlCollection::Complete || held.is_null() {
            return Ok(state);
        }
        if held["nodes"].as_array().map_or(0, Vec::len) >= need {
            return Ok(GraphqlCollection::Partial);
        }
        if connection
            .total
            .is_some_and(|total| pr.pointer(total).is_some_and(Value::is_u64))
        {
            return Ok(GraphqlCollection::Incomplete);
        }
        let Some(cursor) = held
            .pointer("/pageInfo/endCursor")
            .and_then(Value::as_str)
            .filter(|_| pages < max_pages)
            .map(str::to_owned)
        else {
            return Ok(GraphqlCollection::Incomplete);
        };
        let mut next = variables.clone();
        next[connection.variable] = json!(CONNECTION_PAGE);
        next["after"] = json!(cursor);
        let mut answer = match source
            .execute_pull_request_graphql(&connection.continuation_document(), next, context)
            .await
        {
            Ok(answer) => answer,
            Err(error) if error.is_cancelled_or_timed_out() => {
                return Err(error);
            }
            Err(_) => return Ok(GraphqlCollection::Incomplete),
        };
        if !answer.errors.is_empty() {
            errors.push(describe_errors(&answer.errors));
            return Ok(GraphqlCollection::Incomplete);
        }
        let Some(page) = answer
            .data
            .as_mut()
            .and_then(|data| {
                data.pointer_mut(&format!("/repository/pullRequest/{}", connection.key))
            })
            .filter(|page| page.is_object())
        else {
            return Ok(GraphqlCollection::Incomplete);
        };
        let nodes = match page.get_mut("nodes").map(Value::take) {
            Some(Value::Array(nodes)) if !nodes.is_empty() => nodes,
            _ => return Ok(GraphqlCollection::Incomplete),
        };
        let held = &mut pr[connection.key];
        if let Some(list) = held.get_mut("nodes").and_then(Value::as_array_mut) {
            list.extend(nodes);
        }
        held["pageInfo"] = page["pageInfo"].take();
        pages += 1;
    }
}
