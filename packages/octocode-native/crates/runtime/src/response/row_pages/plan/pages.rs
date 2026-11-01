//! Page assignment of a row plan: rows in order, cut at row boundaries.
use super::super::{Placed, row_part_chars};
use std::collections::BTreeSet;

/// A row's first-fragment bound: unbounded when the row fits a page whole
/// (it is never split: pages cut at row boundaries), else the `room` left
/// on the page it starts on, less the row's comma, per-call facts and
/// `rowPart`. Only a row larger than a page is split into parts.
pub(super) fn head_limit(size: usize, room: Room, reserve: usize) -> usize {
    if size <= room.page {
        usize::MAX
    } else {
        room.left
            .saturating_sub(1 + reserve + row_part_chars(1, size.max(1)))
            .max(1)
    }
}

/// Room on a page: a whole page's `row_budget`, and what is `left` of the
/// page the next fragment goes on.
#[derive(Clone, Copy)]
pub(super) struct Room {
    page: usize,
    left: usize,
}

/// Pages assigned in row order: a page takes fragments while they fit
/// `row_budget`, and never two parts of one row, so the entry for a row on
/// a page is all of that row the page holds. Each page is one call a reader
/// makes, so the page count is the response's `totalPages`.
pub(super) struct Pages {
    row_budget: usize,
    placed: Vec<Placed>,
    page: usize,
    page_chars: usize,
    page_rows: BTreeSet<usize>,
}

impl Pages {
    pub(super) fn new(row_budget: usize) -> Self {
        Self {
            row_budget,
            placed: Vec::new(),
            page: 0,
            page_chars: 0,
            page_rows: BTreeSet::new(),
        }
    }

    /// The room the next row starts in: what is left of the current page,
    /// or a whole page when it is empty or full.
    pub(super) fn room(&self) -> Room {
        let left = self.row_budget.saturating_sub(self.page_chars);
        Room {
            page: self.row_budget,
            left: if self.page_rows.is_empty() || left == 0 {
                self.row_budget
            } else {
                left
            },
        }
    }

    pub(super) fn place(&mut self, mut fragment: Placed, chars: usize) {
        if !self.page_rows.is_empty()
            && (self.page_rows.contains(&fragment.row) || self.page_chars + chars > self.row_budget)
        {
            self.page += 1;
            self.page_chars = 0;
            self.page_rows.clear();
        }
        self.page_chars += chars;
        self.page_rows.insert(fragment.row);
        fragment.page = self.page;
        self.placed.push(fragment);
    }

    /// The placed fragments and the page count.
    pub(super) fn finish(self) -> (Vec<Placed>, usize) {
        (self.placed, self.page + 1)
    }
}
