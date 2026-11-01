//! One listing window cut into pages, and the continuations it leads to
//! (`nextPage`, `continueWalk`, `narrowScope`).
use super::{continuation, narrow_scope};
use serde_json::{Value, json};

/// Serialized chars one row adds to a page: `entry` always, plus `header`
/// when the row opens a group: it starts the page, or `continues` is false
/// (it follows a row of another group). A page repeats the header of a group
/// it continues, so every page reads on its own.
#[derive(Clone, Copy, Debug)]
pub(super) struct RowCost {
    pub(super) entry: usize,
    pub(super) header: usize,
    pub(super) continues: bool,
}

/// Row ranges of each page: as many rows as fit `budget` serialized chars,
/// and at most `limit` rows when set (at least one per page). Costs come
/// from snapshot-bound data, so every page of one snapshot cuts at the same
/// rows. An empty listing has no pages.
pub(super) fn page_ranges(
    costs: &[RowCost],
    budget: usize,
    limit: Option<usize>,
) -> Vec<std::ops::Range<usize>> {
    let limit = limit.unwrap_or(usize::MAX).max(1);
    let total = costs.len();
    if total == 0 {
        return Vec::new();
    }
    let mut pages = Vec::new();
    let mut start = 0;
    let mut used = 0usize;
    for (index, cost) in costs.iter().enumerate() {
        let opening = cost.entry.saturating_add(cost.header);
        let added = if cost.continues { cost.entry } else { opening };
        if index > start && (used.saturating_add(added) > budget || index - start >= limit) {
            pages.push(start..index);
            start = index;
            used = 0;
        }
        used = used.saturating_add(if index == start { opening } else { added });
    }
    pages.push(start..total);
    pages
}

/// Where a listing's rows were cut, for the continuation tail both
/// operations share.
pub(super) struct Cut {
    pub(super) page: usize,
    pub(super) total_pages: usize,
    /// Rows in the listing after `limit`.
    pub(super) total: usize,
    pub(super) requested: usize,
    /// Rows the walk returned before `limit` cut them.
    pub(super) available: usize,
    /// Rows past this window's `limit` remain: `next.continueWalk`
    /// lists them in the next window.
    pub(super) limit_cut: bool,
    /// The walk ran out of time before this root-relative entry: it and
    /// everything after it in walk order is unscanned.
    pub(super) time_cut: Option<String>,
    /// How the window's rows end at a time cut.
    pub(super) time_window: TimeWindow,
    /// Walk entries inspected, `walkOffset` ones included: where
    /// `next.continueWalk` resumes a time-cut walk.
    pub(super) walk_position: usize,
    /// Walk entries earlier windows inspected (`walkOffset`).
    pub(super) walk_offset: usize,
    /// The walk stopped at the first match past the limit, so only a lower
    /// bound of the total is known.
    pub(super) early_exit: bool,
    pub(super) total_discovered: usize,
    /// Listing rows earlier windows listed (`scanOffset`).
    pub(super) scan_offset: usize,
    /// Listing rows before this window's end: the next window's
    /// `scanOffset`.
    pub(super) covered: usize,
    pub(super) snapshot: String,
}

impl Cut {
    pub(super) fn has_more(&self) -> bool {
        self.page < self.total_pages
    }
    /// The `walkOffset` that resumes a walk cut by time after the last entry
    /// its window covers, when no row of this window was dropped, the
    /// listing is in walk order wherever `scanOffset` skips rows, and the
    /// walk advanced past its own resume point.
    pub(super) fn resume_at(&self) -> Option<usize> {
        (self.time_cut.is_some()
            && !self.limit_cut
            && self.time_window != TimeWindow::SortedPastScanOffset
            && self.walk_position > self.walk_offset)
            .then_some(self.walk_position)
    }
    /// The listing rows `scanOffset` skipped that this window's walk did
    /// not reach: the resumed walk skips them before it lists.
    pub(super) fn rows_left_to_skip(&self) -> usize {
        self.scan_offset.saturating_sub(self.available)
    }
    /// The time cut has no continuation: no resume reaches the entries past
    /// it.
    pub(super) fn time_terminal(&self) -> bool {
        self.time_cut.is_some() && self.resume_at().is_none()
    }
    pub(super) fn out_of_range(&self) -> bool {
        self.total > 0 && self.page > self.total_pages
    }

    /// The page, scan-expansion and partial-coverage continuations.
    pub(super) fn finish(
        &self,
        out: &mut Value,
        query: &impl serde::Serialize,
        warnings: &mut Vec<String>,
    ) {
        if self.has_more() {
            out["calls"]["nextPage"] = continuation(
                query,
                json!({"page": self.page + 1, "snapshot": self.snapshot}),
                true,
            );
        }
        if self.time_terminal() {
            let past = if self.time_window == TimeWindow::SortedPastScanOffset {
                format!(
                    "the walk stopped at its time budget before `{}` and this listing's sort order cannot skip the rows of an earlier window across a cut walk, so this window lists none; its rows and entries from there on in walk order (name-sorted, depth-first)",
                    self.time_cut.as_deref().unwrap_or_default()
                )
            } else {
                format!(
                    "the walk stopped at its time budget before `{}`; entries from there on in walk order (name-sorted, depth-first)",
                    self.time_cut.as_deref().unwrap_or_default()
                )
            };
            warnings.push(format!("terminalLimit: {past} are on no page."));
            out["calls"]["narrowScope"] = narrow_scope(query);
        }
        if let Some((name, changes)) = self.walk_lead() {
            out["calls"][name] = continuation(query, changes, true);
        }
        // A time cut something continues is no terminal limit: the page
        // that carries `continueWalk` (the window's last) names the entry
        // and that continuation; the window's earlier pages continue with
        // `nextPage` and say only `walkDeadline` (below), so the note is
        // stated once per window.
        if let Some(at) = self
            .time_cut
            .as_deref()
            .filter(|_| !self.time_terminal() && !self.has_more())
        {
            warnings.push(match self.time_window {
                TimeWindow::EndedAtRows => format!(
                    "the walk stopped at its time budget before `{at}` and found more rows than its {}-entry walk window: this window lists the first rows in walk order (name-sorted, depth-first), and entries after the last listed one are not yet listed: `next.continueWalk` resumes the walk there.",
                    self.requested
                ),
                _ => format!(
                    "the walk stopped at its time budget before `{at}`; entries from there on in walk order (name-sorted, depth-first) are not yet listed: `next.continueWalk` resumes the walk there."
                ),
            });
        }
        for (cut, reason) in [
            (
                self.limit_cut,
                crate::response::paging::PartialReason::WalkWindow,
            ),
            (
                self.time_cut.is_some(),
                crate::response::paging::PartialReason::WalkDeadline,
            ),
        ] {
            if cut {
                crate::response::paging::mark_partial(out, reason, &[]);
            }
        }
        if self.time_terminal() {
            crate::response::paging::terminal_limit(
                out,
                crate::response::paging::PartialReason::WalkDeadline,
                &[],
            );
        }
        if self.limit_cut {
            out["clipped"] = json!(true);
            if self.early_exit {
                out["atLeast"] = json!(self.scan_offset + self.total + 1);
            } else {
                out["totalAvailable"] = json!(self.matched());
            }
        }
    }

    /// The one continuation of the walk past this window, offered on the
    /// window's last page (its own pages come first): the lead's name and
    /// the query changes, always `continueWalk`. A window cut at its
    /// walk window lists the next window (the rows after the ones this
    /// window listed, `scanOffset`); a
    /// window cut by time resumes (the walk from the entry after the last
    /// one it inspected). Neither has a ceiling. One window has at most
    /// one: a time cut resumes only a window no limit cut ([`Self::resume_at`]).
    pub(super) fn walk_lead(&self) -> Option<(&'static str, Value)> {
        if self.has_more() {
            return None;
        }
        if self.limit_cut {
            return Some((
                "continueWalk",
                json!({"scanOffset": (self.covered > 0).then_some(self.covered), "page": 1, "snapshot": null}),
            ));
        }
        let position = self.resume_at()?;
        let skip = self.rows_left_to_skip();
        Some((
            "continueWalk",
            json!({"walkOffset": position, "page": 1, "snapshot": null, "scanOffset": (skip > 0).then_some(skip)}),
        ))
    }

    /// Entries the walk matched.
    pub(super) fn matched(&self) -> usize {
        self.total_discovered.max(self.available)
    }
}

/// How a window cut by time ends: whether `next.continueWalk` can resume it
/// without a gap or a repeat, and where.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum TimeWindow {
    /// The window lists every row its walk found (past `scanOffset`): the
    /// walk resumes at its last inspected entry.
    #[default]
    Whole,
    /// The walk found more rows than `limit`: the window ends at the
    /// last row it lists, in walk order, and the walk resumes there. The
    /// rows after it in walk order are not dropped, the resume walks them.
    EndedAtRows,
    /// A listing sorted by something other than walk order resumed with
    /// `scanOffset` skips rows of an earlier window's sort order, which a
    /// walk cut short ranks differently: no row of the window is listed,
    /// and none resumes it.
    SortedPastScanOffset,
}
