use serde_json::{Map, Value, json};

/// One list page's facts. Every tool builds its `pagination` block from these
/// (never a hand-written JSON block), so the key set and order are the one
/// pagination shape on every tool and every page: `unit`, `offset` (where the
/// page starts in the list, from 0), `length` (the units it holds), `total`
/// when the list's size is known, `hasMore`. A page requested past the end
/// adds `outOfRange`, which the response stage turns into the row's
/// `status:"outOfRange"` (`response::pages::state_out_of_range`). `total`
/// counts the paged unit only, never another count of the same result.
/// Tool-specific facts (`countScope`) follow on the block the caller extends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFacts {
    unit: &'static str,
    offset: usize,
    length: usize,
    total: Option<usize>,
    has_more: bool,
    out_of_range: bool,
}

impl PageFacts {
    /// A page of `length` items from `offset` of a list whose size is
    /// unknown (a provider that does not count, or a walk that stops early).
    pub fn open(offset: usize, length: usize, has_more: bool) -> Self {
        Self {
            unit: "items",
            offset,
            length,
            total: None,
            has_more,
            out_of_range: false,
        }
    }

    /// A page of `length` items from `offset` of a list of `total`:
    /// `hasMore` follows from it.
    pub fn counted(offset: usize, length: usize, total: usize) -> Self {
        Self {
            has_more: offset.saturating_add(length) < total,
            ..Self::open(offset, length, false).with_total(total)
        }
    }

    /// Count page `page` (1-based; 0 reads as the first) of `size` items each
    /// over a list of `total`. A page past the end is empty at the end and
    /// flagged `outOfRange`; page 1 of an empty list is the empty list itself.
    pub fn nth(page: usize, size: usize, total: usize) -> Self {
        let window = crate::response::page_windows::count_window(page, size, total);
        Self::counted(window.start, window.len(), total)
            .out_of_range(page.max(1) > crate::response::page_windows::count_pages(total, size))
    }

    /// The start of the count page `page` (1-based; 0 reads as the first) of
    /// `size` items each, for a list whose size is unknown.
    pub fn start_of(page: usize, size: usize) -> usize {
        page.max(1).saturating_sub(1).saturating_mul(size.max(1))
    }

    /// A section before its first page: none of its `total` items shown,
    /// every one still to read.
    pub fn unshown(total: usize) -> Self {
        Self::counted(0, 0, total)
    }

    /// The same page counting `unit` (`files`, `matches`) instead of items.
    pub fn with_unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }

    /// The same page with the size of its whole list, exact or a floor (the
    /// caller's `countScope` says which).
    pub fn with_total(mut self, total: usize) -> Self {
        self.total = Some(total);
        self
    }

    /// The same page, flagged as requested past the end (`outOfRange`).
    pub fn out_of_range(mut self, past_end: bool) -> Self {
        self.out_of_range = past_end;
        self
    }

    /// Whether the page lies past the end of its list.
    pub fn is_past_end(self) -> bool {
        self.out_of_range
    }

    /// The `pagination` block.
    pub fn to_value(self) -> Value {
        let mut block = Map::new();
        block.insert("unit".into(), json!(self.unit));
        block.insert("offset".into(), json!(self.offset));
        block.insert("length".into(), json!(self.length));
        if let Some(total) = self.total {
            block.insert("total".into(), json!(total));
        }
        block.insert("hasMore".into(), json!(self.has_more));
        if self.out_of_range {
            block.insert("outOfRange".into(), json!(true));
        }
        Value::Object(block)
    }
}

/// One content window's facts, beside [`PageFacts`] for count pages: every
/// window block (a read's `pagination`, a body's `sectionPagination` entry,
/// a patch's `patchPagination`) is built from these, so the keys and their
/// order are one shape: `unit` (`lines`, `bytes` or `chars`), `offset`,
/// `length`, `total`, `hasMore`, `nextOffset` while more is left (the shaping
/// stages read it; publication drops it, `next.*` carries the cursor), and
/// `outOfRange` on a window requested past the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanFacts {
    unit: &'static str,
    offset: usize,
    length: usize,
    total: usize,
    states_total: bool,
    states_next: bool,
    out_of_range: bool,
    /// Whether units are left, as the walk that cut the window knows it
    /// ([`Self::walked`]); `None`: the window is one run of a view read
    /// from 0 to `total` in offset order, so the units after it are left.
    walk_has_more: Option<bool>,
}

impl SpanFacts {
    /// The window of up to `length` `unit`s from `offset` over a view of
    /// `total`, clamped to the view (past the end it is empty at the end).
    pub fn window(unit: &'static str, offset: usize, length: usize, total: usize) -> Self {
        let offset = offset.min(total);
        Self {
            unit,
            offset,
            length: length.min(total - offset),
            total,
            states_total: true,
            states_next: true,
            out_of_range: false,
            walk_has_more: None,
        }
    }

    /// The same window of a walk that does not read its view from 0 to
    /// `total` in offset order (a selection's byte page placed in its
    /// source; a long line whose matches are shown before its other bytes):
    /// the walk states whether more is left, and its cursor, never an offset
    /// after the window, says where it goes on (no `nextOffset`).
    pub fn walked(mut self, has_more: bool) -> Self {
        self.walk_has_more = Some(has_more);
        self.states_next = false;
        self
    }

    /// The window of one part of a stream (a file inside a patch page):
    /// coordinates in that part alone, with no `nextOffset`, because the
    /// cursor that continues is the stream's, never the part's.
    pub fn local(unit: &'static str, offset: usize, length: usize, total: usize) -> Self {
        Self {
            states_next: false,
            ..Self::window(unit, offset, length, total)
        }
    }

    /// The same window without its `total`, for a view whose total the row
    /// already states (a line read's `totalLines`).
    pub fn without_total(mut self) -> Self {
        self.states_total = false;
        self
    }

    /// The same window, flagged as requested past the end (`outOfRange`).
    pub fn out_of_range(mut self, past_end: bool) -> Self {
        self.out_of_range = past_end;
        self
    }

    /// Whether units are left after the window.
    pub fn has_more(self) -> bool {
        self.walk_has_more
            .unwrap_or(self.offset + self.length < self.total)
    }

    /// Where the next window starts, while units are left; `None` for a
    /// [`Self::walked`] window, whose cursor names where it goes on.
    pub fn next_offset(self) -> Option<usize> {
        (self.walk_has_more.is_none() && self.has_more()).then_some(self.offset + self.length)
    }

    /// The window block.
    pub fn to_value(self) -> Value {
        let mut block = Map::new();
        block.insert("unit".into(), json!(self.unit));
        block.insert("offset".into(), json!(self.offset));
        block.insert("length".into(), json!(self.length));
        if self.states_total {
            block.insert("total".into(), json!(self.total));
        }
        block.insert("hasMore".into(), json!(self.has_more()));
        if self.states_next
            && let Some(next) = self.next_offset()
        {
            block.insert("nextOffset".into(), json!(next));
        }
        if self.out_of_range {
            block.insert("outOfRange".into(), json!(true));
        }
        Value::Object(block)
    }
}

#[cfg(test)]
mod tests;
