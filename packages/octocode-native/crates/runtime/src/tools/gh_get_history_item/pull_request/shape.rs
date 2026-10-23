//! The shaped pull-request row: metadata, body, file, comment, review and
//! commit sections, and the read's own next steps.
use super::PrPlan;
use super::files::{FileOutcome, shape_files_section};
use super::head_sources::head_sources;
use super::load::PrLoads;
use crate::tools::gh_get_history_item::axis::Axis;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::pr_finish::{pr_metadata, slim_row};
use crate::tools::gh_get_history_item::pr_menu::{INVENTORY_ALL_PATCHES_FILES, pr_next_menu};
use crate::tools::gh_get_history_item::pr_sections::{
    CommentSection, shape_pr_comments, shape_pr_commits, shape_pr_reviews,
};
use crate::tools::gh_get_history_item::remaining::{BodyWindows, Remainder};
use crate::tools::gh_get_history_item::text::paginate_text;
use crate::tools::gh_get_history_item::util::map_comments;
use crate::tools::gh_get_history_item::window::{Loaded, reconcile_collection_total};
use crate::tools::gh_get_history_item::{HistoryItemRequest, Section, Sections};
use octocode_contract_types::ExtraKind;
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Map, Value, json};

/// The read's own next steps. A summary offers the PR-read menu; an
/// unfiltered first inventory page offers its review pick (and every patch
/// on a small PR); a filtered inventory or a patch read is a targeted answer
/// and keeps only its continuations.
fn read_menu(
    query: &HistoryItemRequest,
    review: &[String],
    raw: &Value,
    slim: bool,
    file_read: bool,
) -> Option<Value> {
    if !slim {
        return Some(pr_next_menu(query, review, raw));
    }
    let first_inventory = file_read
        && !query.sections().has(Section::Patches)
        && !gh_get_history_item_query::later_page(query)
        && !gh_get_history_item_query::has_file_filter(query);
    if !first_inventory {
        return None;
    }
    let all_patches = raw
        .get("changed_files")
        .and_then(Value::as_u64)
        .is_some_and(|files| files <= INVENTORY_ALL_PATCHES_FILES);
    let mut next = pr_next_menu(query, review, raw);
    if let Some(menu) = next.as_object_mut() {
        menu.retain(|name, _| {
            name == "readSelectedPatches" || (all_patches && name == "readPatches")
        });
    }
    Some(next).filter(|next| next.as_object().is_some_and(|next| !next.is_empty()))
}

/// A shaped pull-request row and what the response still needs from it.
pub(in crate::tools::gh_get_history_item) struct ShapedPr {
    pub(in crate::tools::gh_get_history_item) row: Value,
    pub(in crate::tools::gh_get_history_item) files: FileOutcome,
    pub(in crate::tools::gh_get_history_item) menu: Option<Value>,
    /// The code the page's anchored review comments discuss, ten ranges a read.
    pub(in crate::tools::gh_get_history_item) comment_reads: Vec<Value>,
    /// What the row's pages leave to read beside its public pages.
    pub(in crate::tools::gh_get_history_item) remainder: Remainder,
    /// Each listed commit's next file page (`continueCommitFiles…`).
    pub(in crate::tools::gh_get_history_item) commit_file_pages: Vec<(String, Value)>,
}

pub(super) async fn shape_pr(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &Sections,
    plan: &PrPlan,
    loads: PrLoads,
) -> Result<ShapedPr, ProviderError> {
    let PrLoads {
        raw,
        files,
        discussion,
        inline,
        reviews,
        commits,
    } = loads;
    let patches = wants.has(Section::Patches);
    let slim = (gh_get_history_item_query::later_page(query)
        || gh_get_history_item_query::has_sections(query))
        && !gh_get_history_item_query::wants(query, ExtraKind::Details);
    let mut row = pr_metadata(&raw, query);
    if slim {
        slim_row(&mut row, query);
    }
    let mut pagination = Map::new();
    let mut remainder = Remainder::default();
    if wants.has(Section::Body) {
        shape_body(&mut row, &mut pagination, &raw, query);
    }
    let widened;
    let query = match &files {
        Some(loaded) if patches => {
            match head_sources(transport, query, context, &raw, loaded, plan).await {
                Some(sources) => {
                    widened = HistoryItemRequest {
                        head_sources: Some(std::sync::Arc::new(sources)),
                        ..query.clone()
                    };
                    &widened
                }
                None => query,
            }
        }
        _ => query,
    };
    let file_outcome = files.map_or_else(FileOutcome::default, |loaded| {
        shape_files_section(&mut row, &mut pagination, loaded, &raw, query, wants, plan)
    });
    remainder.unfinished_patches(file_outcome.unfinished_patches);
    let comment_reads = shape_comment_sections(
        &mut row,
        &mut pagination,
        &mut remainder,
        inline,
        discussion,
        query,
    );
    let mut review_bodies = BodyWindows::default();
    if let Some(loaded) = reviews {
        shape_pr_reviews(
            &mut row,
            &mut pagination,
            loaded.items,
            loaded.state,
            query,
            &mut review_bodies,
        );
    }
    remainder.bodies(Axis::ReviewBody, review_bodies, &mut pagination);
    let mut commit_file_pages = Vec::new();
    if let Some(loaded) = commits {
        let listed = loaded.state.skipped.saturating_add(loaded.items.len());
        let state = loaded.state;
        commit_file_pages = shape_pr_commits(
            transport,
            &mut row,
            &mut pagination,
            loaded.items,
            loaded.state,
            query,
            context,
        )
        .await?;
        let total = raw
            .get("commits")
            .and_then(Value::as_u64)
            .and_then(|total| usize::try_from(total).ok());
        if let Some(page) = pagination.get_mut("commits")
            && reconcile_collection_total(page, state, listed, total, "providerCommitListLimit")
        {
            // The slim section header must keep the authoritative denominator
            // when the provider's returned collection cannot reach it.
            row["commitsCount"] = raw["commits"].clone();
        }
    }
    let menu = read_menu(query, &file_outcome.review, &raw, slim, wants.lists_files());
    if slim && patches {
        drop_named_header(&mut row);
    }
    if !pagination.is_empty() {
        row["contentPagination"] = Value::Object(pagination);
    }
    Ok(ShapedPr {
        row,
        files: file_outcome,
        menu,
        comment_reads,
        remainder,
        commit_file_pages,
    })
}

/// The body's verbatim text window on `row` and its page.
fn shape_body(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    raw: &Value,
    query: &HistoryItemRequest,
) {
    let body = raw.get("body").and_then(Value::as_str).unwrap_or("");
    let (text, page) = paginate_text(
        body,
        gh_get_history_item_query::char_offset(query),
        gh_get_history_item_query::char_length(query),
    );
    row["body"] = json!(text);
    pagination.insert(Axis::Body.key().into(), page);
}

/// Patch rows are the evidence of a section patch read: it re-proves the
/// PR's name (number, title, author), state, and the head it read. The
/// metadata read dates the PR (createdAt); a read without patch rows keeps
/// it.
fn drop_named_header(row: &mut Value) {
    let patch_rows = row["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty() && files.iter().all(Value::is_object));
    if patch_rows
        && row.get("headSha").is_some()
        && let Some(fields) = row.as_object_mut()
    {
        fields.remove("createdAt");
    }
}

/// The inline review comments (`reviewComments`) and the discussion
/// (`comments`), each one page of its own axis; their cut bodies share the
/// one `commentBody` window. Returns the review comments' code reads.
fn shape_comment_sections(
    row: &mut Value,
    pagination: &mut Map<String, Value>,
    remainder: &mut Remainder,
    inline: Option<Loaded>,
    discussion: Option<Loaded>,
    query: &HistoryItemRequest,
) -> Vec<Value> {
    let mut bodies = BodyWindows::default();
    let mut reads = Vec::new();
    for (section, loaded) in [
        (CommentSection::ReviewInline, inline),
        (CommentSection::Discussion, discussion),
    ] {
        let Some(loaded) = loaded else { continue };
        reads.extend(shape_pr_comments(
            row,
            pagination,
            section,
            map_comments(loaded.items),
            loaded.state,
            query,
            &mut bodies,
        ));
    }
    remainder.bodies(Axis::CommentBody, bodies, pagination);
    reads
}
