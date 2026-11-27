//! Promotion of a shaped page's cursors (`sectionPagination`) to `next.*`
//! for pull requests and issues.
use super::axis::{Axis, Cursor};
use super::patch_hop::*;
use super::pr_menu::*;
use super::remaining::Remainder;
use super::{HistoryItemRequest, ItemOperation, Sections};
use crate::gh_get_history_item::sections::Section;
use octocode_runtime_base::id::ToolId;
use serde_json::{Map, Value, json};

/// A provider limit an item read's section hit (GitHub lists at most 3,000
/// PR files), stated on the row as evidence beside the section's page block.
fn hoist_provider_limit(out: &mut Value, row: &str) {
    let limit = out
        .pointer(&format!("{row}/sectionPagination"))
        .and_then(Value::as_object)
        .and_then(|pages| pages.values().find_map(|page| page.get("providerLimit")))
        .cloned();
    // A provider limit names its reason (the contract requires it).
    let Some((limit, reason)) = limit.and_then(|limit| {
        let reason = limit
            .get("reason")?
            .as_str()?
            .parse::<octocode_runtime_base::response::paging::PartialReason>()
            .ok()?;
        Some((limit, reason))
    }) else {
        return;
    };
    octocode_runtime_base::response::paging::terminal_limit(out, reason, &[]);
    out["providerLimit"] = limit;
}

/// Page blocks state where the page stands; the cursor rides the `next`
/// continuation it was read into, never the block.
fn strip_cursors(pages: &mut Map<String, Value>) {
    for page in pages.values_mut().filter_map(Value::as_object_mut) {
        for cursor in [Cursor::Page, Cursor::Offset] {
            page.remove(cursor.field());
        }
    }
}

/// The text axis whose windows a list's next page waits for: the patches
/// of a file page, the comment bodies of a comment or review-comment page,
/// the review bodies of a review page.
fn cut_text(list: Axis) -> Option<Axis> {
    match list {
        Axis::Files => Some(Axis::Patches),
        Axis::Comments | Axis::ReviewComments => Some(Axis::CommentBody),
        Axis::Reviews => Some(Axis::ReviewBody),
        Axis::Body | Axis::ReviewBody | Axis::Patches | Axis::CommentBody | Axis::Commits => None,
    }
}

/// The lists whose next page waits on this page: the text their page cuts
/// has more windows. The window that finishes that text offers the next
/// page, so each page is offered once.
fn waiting_lists(pages: &Map<String, Value>) -> Vec<String> {
    let open = |axis: Axis| {
        pages
            .get(axis.key())
            .is_some_and(|entry| entry.get("hasMore").and_then(Value::as_bool) == Some(true))
    };
    pages
        .keys()
        .filter(|key| Axis::parse(key).and_then(cut_text).is_some_and(open))
        .cloned()
        .collect()
}

/// `next.*` for every unfinished axis of the pull request's page, and the
/// `isPartial` is set only for an axis
/// with more to read that no continuation reaches: `hasMore` beside a
/// `next.*` page already says the rest is unread.
pub(super) fn promote_pr_continuations(out: &mut Value, q: &HistoryItemRequest) {
    hoist_provider_limit(out, "/pullRequests/0");
    let Some(pages) = out
        .pointer_mut("/pullRequests/0/sectionPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    // A page with unread patches or bodies continues them first: the
    // window that finishes them offers the next page of their list.
    let waiting = waiting_lists(pages);
    let mut unreached = false;
    for (key, entry) in pages.iter_mut() {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) || waiting.contains(key) {
            continue;
        }
        let axis = Axis::parse(key);
        let Some((axis, cursor)) =
            axis.and_then(|axis| Some((axis, entry.get(axis.cursor().field())?.clone())))
        else {
            unreached = true;
            continue;
        };
        let mut nq = base_public_query(q, ItemOperation::PullRequest);
        nq[axis.cursor().query_key()] = cursor;
        let call = if axis == Axis::Patches {
            patch_continuation(nq, entry, q)
        } else {
            narrow_to_axis(&mut nq, axis, q);
            if axis == Axis::Files {
                pin_file_page_size(&mut nq, q, q.sections().has(Section::Patches));
            }
            continuation(nq)
        };
        if let Some(call) = call {
            octocode_runtime_base::response::collection::insert_walk(
                &mut entry["next"],
                ToolId::GhGetHistoryItem,
                None,
                call,
            );
        }
    }
    strip_cursors(pages);
    if unreached {
        octocode_runtime_base::response::paging::set_partial(out, true);
    }
}

/// A pull-request continuation of `axis` reads that surface alone. `offset`
/// is a single field shared by the body, review-body, comment-body and patch
/// windows: a char-window continuation narrows content to the surface it
/// continues, and a page continuation restarts every char window, so one
/// surface's offset never skews another's.
fn narrow_to_axis(nq: &mut Value, axis: Axis, q: &HistoryItemRequest) {
    match axis {
        Axis::Body => {
            // The body has no page: a section page cursor would be inert.
            Sections::of([Section::Body]).set_on(nq);
            remove_key(nq, "page");
        }
        Axis::ReviewBody => Sections::of([Section::Reviews]).set_on(nq),
        Axis::CommentBody => {
            let read = q.sections();
            let comments = Sections::of(
                [Section::Comments, Section::ReviewComments]
                    .into_iter()
                    .filter(|section| read.has(*section)),
            );
            let comments = if comments.is_empty() {
                Sections::of([Section::Comments])
            } else {
                comments
            };
            comments.set_on(nq);
        }
        Axis::Patches => {}
        Axis::Files | Axis::Comments | Axis::ReviewComments | Axis::Reviews | Axis::Commits => {
            remove_key(nq, Cursor::Offset.query_key());
            // A collection page continues only its own surface; the other
            // surfaces were already delivered on this call. The discussion
            // and the inline review comments page apart.
            // A page of patches lists their files: `patches` alone stays
            // the caller's one spelling of that read.
            let read = q.sections();
            let own = axis.own_sections();
            Sections::of(own.iter().copied().filter(|section| read.has(*section))).set_on(nq);
        }
    }
}

/// `next.*` for every unfinished axis of the issue's page, and the note of
/// `isPartial` as for a pull request
/// ([`promote_pr_continuations`]).
pub(super) fn promote_issue_continuations(
    out: &mut Value,
    q: &HistoryItemRequest,
    remainder: &Remainder,
) {
    let Some(pages) = out
        .pointer_mut("/issues/0/sectionPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let comments = Sections::of([Section::Comments]);
    // A comment-body hop keeps the body section its first window read: the
    // hop does not show the body again, but sizes its page like that window.
    let comment_bodies = if q.sections().has(Section::Body) {
        comments.with(Section::Body)
    } else {
        comments
    };
    // Cut comment bodies continue first: the window that finishes them
    // offers the next comment page.
    let waiting = waiting_lists(pages);
    let mut unreached = false;
    for (key, entry) in pages.iter_mut() {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) || waiting.contains(key) {
            continue;
        }
        let (axis, sections) = match Axis::parse(key) {
            Some(Axis::Body) => (Axis::Body, Sections::of([Section::Body])),
            Some(Axis::CommentBody) => (Axis::CommentBody, comment_bodies),
            Some(Axis::Comments) => (Axis::Comments, comments),
            _ => {
                unreached = true;
                continue;
            }
        };
        let Some(cursor) = entry.get(axis.cursor().field()) else {
            unreached = true;
            continue;
        };
        let mut nq = base_public_query(q, ItemOperation::Issue);
        nq[axis.cursor().query_key()] = cursor.clone();
        sections.set_on(&mut nq);
        if axis == Axis::Comments {
            // A comment page cut to the response budget resumes with the
            // page size that starts a page at its first unshown comment.
            if let Some(size) = remainder.next_page_size() {
                nq["pageSize"] = json!(size);
            }
            // A page restarts every body window: the default offset.
            remove_key(&mut nq, Cursor::Offset.query_key());
        }
        if let Some(call) = continuation(nq) {
            octocode_runtime_base::response::collection::insert_walk(
                &mut entry["next"],
                ToolId::GhGetHistoryItem,
                None,
                call,
            );
        }
    }
    strip_cursors(pages);
    if unreached {
        octocode_runtime_base::response::paging::set_partial(out, true);
    }
}

#[cfg(test)]
mod tests;
