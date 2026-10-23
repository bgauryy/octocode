//! One listing window cut into pages, and the continuations it leads to
//! (`nextPage`, `continueWalk`, `narrowScope`).
use super::{continuation, max_walk, narrow_scope};
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

/// Row ranges of each page: `page_size` rows each when the caller sets it,
/// else as many rows as fit `budget` serialized chars (at least one per
/// page). Costs come from snapshot-bound data, so every page of one snapshot
/// cuts at the same rows. An empty listing has no pages.
pub(super) fn page_ranges(
    costs: &[RowCost],
    page_size: Option<usize>,
    budget: usize,
) -> Vec<std::ops::Range<usize>> {
    let total = costs.len();
    if total == 0 {
        return Vec::new();
    }
    if let Some(size) = page_size {
        let size = size.max(1);
        return (0..total)
            .step_by(size)
            .map(|start| start..start.saturating_add(size).min(total))
            .collect();
    }
    let mut pages = Vec::new();
    let mut start = 0;
    let mut used = 0usize;
    for (index, cost) in costs.iter().enumerate() {
        let opening = cost.entry.saturating_add(cost.header);
        let added = if cost.continues { cost.entry } else { opening };
        if index > start && used.saturating_add(added) > budget {
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
    /// Rows in the listing after `maxEntries`.
    pub(super) total: usize,
    pub(super) requested: usize,
    /// Rows the walk returned before `maxEntries` cut them.
    pub(super) available: usize,
    pub(super) limit_cut: bool,
    pub(super) scan_cut: bool,
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
    /// The listing ranks rows by something other than walk order.
    pub(super) ranked: bool,
    pub(super) snapshot: String,
}

impl Cut {
    pub(super) fn has_more(&self) -> bool {
        self.page < self.total_pages
    }
    /// More of the walk's own rows fit a wider `maxEntries`.
    pub(super) fn can_expand(&self) -> bool {
        self.limit_cut && self.requested < max_walk() as usize
    }
    /// The `walkOffset` that resumes a walk cut by time after the last entry
    /// its window covers, when no row of this window was dropped (a window
    /// cut at the walk ceiling lost rows no resume re-lists), the listing
    /// is in walk order wherever `scanOffset` skips rows, and the walk
    /// advanced past its own resume point and within the contract.
    pub(super) fn resume_at(&self) -> Option<usize> {
        if self.time_cut.is_none()
            || self.limit_cut
            || self.scan_cut
            || self.time_window == TimeWindow::SortedPastScanOffset
            || self.walk_position <= self.walk_offset
        {
            return None;
        }
        crate::response::paging::admitted(
            crate::id::ToolId::StructureSearch,
            None,
            "walkOffset",
            self.walk_position,
        )
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
    /// More pages exist, but the next page number is past the contract's
    /// `page` maximum.
    pub(super) fn page_ceiling(&self) -> bool {
        self.has_more() && self.next_page().is_none()
    }
    /// The page a continuation names after this one, when the contract
    /// admits it.
    pub(super) fn next_page(&self) -> Option<usize> {
        crate::response::paging::admitted(
            crate::id::ToolId::StructureSearch,
            None,
            "page",
            self.page + 1,
        )
    }
    /// Entries past this walk are on no page through any widening: the
    /// walk stopped at its entry ceiling (`scan_cut`; a wider `maxEntries`
    /// lists more of what it walked, never past it), or at the largest
    /// `maxEntries`.
    pub(super) fn walk_ceiling(&self) -> bool {
        self.scan_cut || (self.limit_cut && !self.can_expand())
    }
    pub(super) fn terminal(&self) -> bool {
        self.page_ceiling() || self.walk_ceiling() || self.time_terminal()
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
        // Every listed row stays reachable: a walk ceiling ends the listing,
        // never the pages over the rows it did list.
        if self.has_more()
            && let Some(next_page) = self.next_page()
        {
            out["next"]["nextPage"] =
                continuation(query, json!({"page": next_page, "snapshot": self.snapshot}));
        }
        if self.terminal() {
            let past = if self.page_ceiling() {
                format!(
                    "page {} is the last page a continuation may request; later rows of this listing",
                    self.page
                )
            } else if self.walk_ceiling() {
                if self.early_exit {
                    format!(
                        "the walk stopped at maxEntries {}; entries past it",
                        self.requested
                    )
                } else if self.scan_cut {
                    // The walk stopped at its ceiling: what it found is a
                    // lower bound, and a sort ranks only what it walked.
                    let ranking = if self.ranked {
                        ", so this sort ranks only the entries walked first (walk order: name-sorted, depth-first)"
                    } else {
                        ""
                    };
                    format!(
                        "the walk stopped at its {}-entry ceiling with at least {} entries matched{ranking}; entries past it",
                        max_walk(),
                        self.matched()
                    )
                } else {
                    format!(
                        "{} entries matched, past maxEntries {}; the rest",
                        self.matched(),
                        self.requested
                    )
                }
            } else if self.time_window == TimeWindow::SortedPastScanOffset {
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
            out["next"]["narrowScope"] = narrow_scope(query);
        }
        if let Some((name, changes)) = self.walk_lead() {
            out["next"][name] = continuation(query, changes);
        }
        // A time cut something continues is no terminal limit: the page
        // names the entry and the continuation that reaches the rest.
        if let Some(at) = self.time_cut.as_deref().filter(|_| !self.time_terminal()) {
            warnings.push(match self.time_window {
                TimeWindow::EndedAtRows => format!(
                    "the walk stopped at its time budget before `{at}` and found more rows than maxEntries {}: this window lists the first rows in walk order (name-sorted, depth-first), and entries after the last listed one are not yet listed: the page's continuation resumes the walk there.",
                    self.requested
                ),
                _ => format!(
                    "the walk stopped at its time budget before `{at}`; entries from there on in walk order (name-sorted, depth-first) are not yet listed: the page's continuation resumes the walk there."
                ),
            });
        }
        for (cut, reason) in [
            (self.limit_cut, "maxEntries"),
            (self.scan_cut, "walkLimit"),
            (self.time_cut.is_some(), "walkDeadline"),
        ] {
            if cut {
                crate::response::paging::mark_partial(out, reason, &[]);
            }
        }
        if self.page_ceiling() {
            crate::response::paging::terminal_limit(
                out,
                crate::response::paging::CURSOR_MAXIMUM,
                &[],
            );
        } else if self.walk_ceiling() {
            // The walk's own cut reason is already listed above.
            let reason = if self.scan_cut {
                "walkLimit"
            } else {
                "maxEntries"
            };
            crate::response::paging::terminal_limit(out, reason, &[]);
        } else if self.time_terminal() {
            crate::response::paging::terminal_limit(out, "walkDeadline", &[]);
        }
        if self.limit_cut || self.scan_cut {
            out["clipped"] = json!(true);
            if self.early_exit {
                out["atLeast"] = json!(self.scan_offset + self.total + 1);
            } else if self.scan_cut {
                out["atLeast"] = json!(self.matched());
            } else {
                out["totalAvailable"] = json!(self.matched());
            }
        }
    }

    /// The one continuation of the walk past this window, offered on the
    /// window's last page (its own pages come first): the lead's name and
    /// the query changes, always `continueWalk`. A window cut at
    /// `maxEntries` widens (a wider walk listing only the rows after the ones
    /// this window listed); a window cut by time resumes (the walk from the
    /// entry after the last one it inspected). One window has at most
    /// one: a time cut resumes only a window no limit cut ([`Self::resume_at`]).
    pub(super) fn walk_lead(&self) -> Option<(&'static str, Value)> {
        if self.has_more() {
            return None;
        }
        if self.can_expand() {
            let wider = self
                .requested
                .saturating_mul(2)
                .max(self.requested + 1)
                .min(max_walk() as usize);
            return Some((
                "continueWalk",
                json!({"maxEntries": wider, "scanOffset": (self.covered > 0).then_some(self.covered), "page": 1, "snapshot": null}),
            ));
        }
        let position = self.resume_at()?;
        let skip = self.rows_left_to_skip();
        Some((
            "continueWalk",
            json!({"walkOffset": position, "page": 1, "snapshot": null, "scanOffset": (skip > 0).then_some(skip)}),
        ))
    }

    /// Entries the walk matched: every one when it walked the whole tree,
    /// a lower bound when it stopped at its ceiling (`scan_cut`).
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
    /// The walk found more rows than `maxEntries`: the window ends at the
    /// last row it lists, in walk order, and the walk resumes there. The
    /// rows after it in walk order are not dropped, the resume walks them.
    EndedAtRows,
    /// A listing sorted by something other than walk order resumed with
    /// `scanOffset` skips rows of an earlier window's sort order, which a
    /// walk cut short ranks differently: no row of the window is listed,
    /// and none resumes it.
    SortedPastScanOffset,
}
