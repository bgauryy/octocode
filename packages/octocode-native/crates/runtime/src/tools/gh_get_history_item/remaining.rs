//! What an item read leaves to read beside its public pages. A shaped page
//! knows counts its public `contentPagination` does not state (the chars left
//! in every body one shared `offset` continues, the provider comments after a
//! cut page, the unfinished patch files of a window, the page size that
//! resumes a cut comment page). They stay typed here, never in an output
//! block, and become the row's [`Remaining`] notes
//! ([`crate::response::result::ToolData::remaining`]).
use super::axis::Axis;
use crate::response::pages::{counted, items_left, more};
use crate::response::result::Remaining;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// The body windows of one page that one shared `offset` continues: the
/// first cut window (the page's public body entry) and the chars left in
/// every cut body.
#[derive(Debug, Default)]
pub(super) struct BodyWindows {
    first: Option<Value>,
    left: u64,
}

impl BodyWindows {
    /// Notes one body's window `page`: a cut window adds what it leaves.
    pub(super) fn add(&mut self, page: &Value) {
        if page["hasMore"] != true {
            return;
        }
        let left = page["total"]
            .as_u64()
            .zip(page["nextOffset"].as_u64())
            .map_or(0, |(total, next)| total.saturating_sub(next));
        self.first.get_or_insert_with(|| page.clone());
        self.left += left;
    }
}

/// Counts a shaped page knows beside its public pages, by axis.
#[derive(Debug, Default)]
pub(super) struct Remainder {
    /// Chars left after the shown windows (`commentBody`, `reviewBody`).
    chars: BTreeMap<Axis, u64>,
    /// Provider items left after the page (an issue's `comments`).
    items: BTreeMap<Axis, u64>,
    /// Files of the patch window not fully delivered.
    unfinished_patches: usize,
    /// The `pageSize` that starts the next issue comment page at its first
    /// unshown comment.
    next_page_size: Option<usize>,
}

impl Remainder {
    /// Puts the page's body `windows` under `axis`: the first cut window is
    /// the public entry, the chars left in all of them stay here.
    pub(super) fn bodies(
        &mut self,
        axis: Axis,
        windows: BodyWindows,
        pagination: &mut Map<String, Value>,
    ) {
        if let Some(first) = windows.first {
            pagination.insert(axis.key().into(), first);
            self.chars.insert(axis, windows.left);
        }
    }

    /// `left` provider items follow the page of `axis`.
    pub(super) fn items(&mut self, axis: Axis, left: u64) {
        self.items.insert(axis, left);
    }

    /// The patch window leaves `files` unfinished.
    pub(super) fn unfinished_patches(&mut self, files: usize) {
        self.unfinished_patches = files;
    }

    /// The next comment page starts at its first unshown comment with
    /// `size` per page.
    pub(super) fn resume_page_size(&mut self, size: usize) {
        self.next_page_size = Some(size);
    }

    pub(super) fn next_page_size(&self) -> Option<usize> {
        self.next_page_size
    }

    /// The note of the page that continues `axis` from its public
    /// `entry`: what is left, worded, when the page counts it.
    pub(super) fn note(&self, axis: Axis, entry: &Value) -> Option<Remaining> {
        Some(Remaining {
            page: axis.next_name().to_owned(),
            left: self.left(axis, entry)?,
        })
    }

    /// What `axis`'s page `entry` leaves to read: `None` when it is done or
    /// the count is unknown.
    fn left(&self, axis: Axis, entry: &Value) -> Option<String> {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        if axis == Axis::Patches && self.unfinished_patches > 0 {
            return Some(counted(
                u64::try_from(self.unfinished_patches).unwrap_or(u64::MAX),
                "unfinished patch",
                "unfinished patches",
            ));
        }
        if let Some(label) = axis.text_label() {
            return self
                .chars_left(axis, entry)
                .map(|left| format!("{left} more {label} chars"));
        }
        let (singular, plural) = axis.noun()?;
        let left = match self.items.get(&axis) {
            Some(left) => Some(*left).filter(|left| *left > 0),
            None => items_left(entry),
        };
        left.map(|left| more(left, singular, plural))
    }

    /// Chars left after `axis`'s shown window: every cut body's rest when
    /// the page counted them, else the window's own `total - nextOffset`.
    fn chars_left(&self, axis: Axis, entry: &Value) -> Option<u64> {
        if let Some(left) = self.chars.get(&axis) {
            return Some(*left).filter(|left| *left > 0);
        }
        let total = entry.get("total")?.as_u64()?;
        let offset = entry.get("nextOffset")?.as_u64()?;
        total.checked_sub(offset).filter(|left| *left > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Axis, Remainder};
    use serde_json::json;

    fn note(
        remainder: &Remainder,
        name: &str,
        axis: Axis,
        entry: serde_json::Value,
    ) -> Option<String> {
        remainder.note(axis, &entry).map(|note| {
            assert_eq!(note.page, name);
            note.left
        })
    }

    /// Each axis words what its page leaves in its own unit; a finished
    /// page or an unknown count words nothing.
    #[test]
    fn each_axis_words_what_its_page_leaves() {
        let mut remainder = Remainder::default();
        remainder.unfinished_patches(12);
        let files = json!({"currentPage": 1, "pageSize": 30, "totalItems": 37, "hasMore": true});
        assert_eq!(
            note(
                &remainder,
                "continuePatch",
                Axis::Patches,
                json!({"hasMore": true})
            )
            .as_deref(),
            Some("12 unfinished patches")
        );
        assert_eq!(
            note(&remainder, "nextFilePage", Axis::Files, files).as_deref(),
            Some("7 more changed files")
        );
        remainder.unfinished_patches(1);
        assert_eq!(
            note(
                &remainder,
                "continuePatch",
                Axis::Patches,
                json!({"hasMore": true})
            )
            .as_deref(),
            Some("1 unfinished patch")
        );
        let body = json!({"unit": "chars", "offset": 0, "length": 12000, "total": 14366,
            "hasMore": true, "nextOffset": 12000});
        assert_eq!(
            note(&Remainder::default(), "continueBody", Axis::Body, body).as_deref(),
            Some("2366 more body chars")
        );
        let reviews = json!({"currentPage": 2, "pageSize": 10, "totalItems": 21, "hasMore": true});
        assert_eq!(
            note(
                &Remainder::default(),
                "nextReviewPage",
                Axis::Reviews,
                reviews
            )
            .as_deref(),
            Some("1 more review")
        );
        let inline = json!({"currentPage": 1, "pageSize": 30, "totalItems": 600, "hasMore": true});
        assert_eq!(
            note(
                &Remainder::default(),
                "nextReviewCommentPage",
                Axis::ReviewComments,
                inline
            )
            .as_deref(),
            Some("570 more review comments")
        );
        for (axis, entry) in [
            (
                Axis::Comments,
                json!({"hasMore": false, "totalItems": 9, "pageSize": 1, "currentPage": 1}),
            ),
            (Axis::Comments, json!({"hasMore": true})),
            (Axis::Patches, json!({"hasMore": true})),
        ] {
            assert_eq!(
                note(&Remainder::default(), axis.next_name(), axis, entry),
                None,
                "{axis:?}"
            );
        }
    }

    /// An issue's comment page counts the provider comments after it,
    /// though its pages do not tile `currentPage × pageSize`.
    #[test]
    fn an_issue_comment_page_counts_the_provider_comments_left() {
        let mut remainder = Remainder::default();
        remainder.items(Axis::Comments, 93);
        remainder.resume_page_size(12);
        assert_eq!(remainder.next_page_size(), Some(12));
        assert_eq!(
            note(
                &remainder,
                "nextCommentPage",
                Axis::Comments,
                json!({"hasMore": true, "totalItems": 105})
            )
            .as_deref(),
            Some("93 more comments")
        );
    }
}
