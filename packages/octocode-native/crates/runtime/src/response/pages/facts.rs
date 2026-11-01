use serde_json::{Map, Value, json};

/// One page's facts. Every tool builds its `pagination` block from these
/// (never a hand-written JSON block), so the key set and order are the same
/// on every tool and every page: `page`, `totalPages` and
/// `totalItems` when the total is known, `pageSize` when pages are cut by
/// count, `hasMore`, and `outOfRange` on a page past the end. `totalItems`
/// counts the paged unit only (the rows the pages split), never another
/// count of the same result. Tool-specific facts (`countScope`, cursors the
/// stage moves to `next`) follow on the block the caller extends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageFacts {
    page: usize,
    page_size: Option<usize>,
    total_items: Option<usize>,
    total_pages: Option<usize>,
    has_more: bool,
    out_of_range: bool,
}

impl PageFacts {
    /// A count-cut page of a known total: the page count and `hasMore`
    /// follow from it.
    pub(crate) fn counted(page: usize, page_size: usize, total_items: usize) -> Self {
        let page = page.max(1);
        let size = page_size.max(1);
        Self {
            page,
            page_size: Some(size),
            total_items: Some(total_items),
            total_pages: Some(crate::response::page_windows::count_pages(
                total_items,
                size,
            )),
            has_more: page.saturating_mul(size) < total_items,
            out_of_range: false,
        }
    }

    /// The page before the first: the section states its `total_items` and
    /// that every page of it is still to read (`hasMore`), with none shown;
    /// it renders `unshown: true` in place of a page number.
    pub(crate) fn unshown(page_size: usize, total_items: usize) -> Self {
        Self {
            page: 0,
            has_more: total_items > 0,
            ..Self::counted(1, page_size, total_items)
        }
    }

    /// A page whose total is unknown (a provider that does not count, or a
    /// walk that stops early): only the cursor facts.
    pub(crate) fn open(page: usize, page_size: Option<usize>, has_more: bool) -> Self {
        Self {
            page: page.max(1),
            page_size,
            total_items: None,
            total_pages: None,
            has_more,
            out_of_range: false,
        }
    }

    /// A page cut by size (bytes, a response budget), not by count: the
    /// pager states the page count and the total of the paged unit.
    pub(crate) fn sized(
        page: usize,
        total_pages: usize,
        total_items: usize,
        has_more: bool,
    ) -> Self {
        Self {
            page: page.max(1),
            page_size: None,
            total_items: Some(total_items),
            total_pages: Some(total_pages.max(1)),
            has_more,
            out_of_range: false,
        }
    }

    /// The same page with a known total of the paged unit. A page without a
    /// page count derives one from its page size; a size-cut page keeps the
    /// count its pager stated.
    pub(crate) fn with_total(mut self, total_items: usize) -> Self {
        self.total_items = Some(total_items);
        if self.total_pages.is_none()
            && let Some(size) = self.page_size
        {
            self.total_pages = Some(crate::response::page_windows::count_pages(
                total_items,
                size,
            ));
        }
        self
    }

    /// The same page with a partial count of the paged unit (items loaded
    /// so far, a provider's capped total): no page count follows from it.
    pub(crate) fn with_items(mut self, total_items: usize) -> Self {
        self.total_items = Some(total_items);
        self
    }

    /// The same size-cut page with the caller's requested page size.
    pub(crate) fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = Some(page_size);
        self
    }

    /// The same page, flagged as requested past the end (`outOfRange`).
    pub(crate) fn out_of_range(mut self, past_end: bool) -> Self {
        self.out_of_range = past_end;
        self
    }

    /// `(page, totalPages)` when the page lies past the last page of a
    /// known count; page 1 of an empty result is the empty result itself.
    pub(crate) fn past_end(self) -> Option<(usize, usize)> {
        let pages = self.total_pages?;
        (self.page > pages.max(1)).then_some((self.page, pages))
    }

    /// The `pagination` block.
    pub(crate) fn to_value(self) -> Value {
        let mut block = Map::new();
        if self.page == 0 {
            block.insert("unshown".into(), json!(true));
        } else {
            block.insert("page".into(), json!(self.page));
        }
        if let Some(pages) = self.total_pages {
            block.insert("totalPages".into(), json!(pages));
        }
        if let Some(size) = self.page_size {
            block.insert("pageSize".into(), json!(size));
        }
        if let Some(total) = self.total_items {
            block.insert("totalItems".into(), json!(total));
        }
        block.insert("hasMore".into(), json!(self.has_more));
        if self.out_of_range {
            block.insert("outOfRange".into(), json!(true));
        }
        Value::Object(block)
    }
}

/// One content window's facts, beside [`PageFacts`] for count pages: every
/// window block (a read's `pagination`, a body's `contentPagination` entry,
/// a patch's `patchPagination`) is built from these, so the keys and their
/// order are one shape: `unit` (`lines`, `bytes` or `chars`), `offset`,
/// `length`, `total`, `hasMore`, `nextOffset` while more is left (the shaping
/// stages read it; publication drops it, `next.*` carries the cursor), and
/// `outOfRange` on a window requested past the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpanFacts {
    unit: &'static str,
    offset: usize,
    length: usize,
    total: usize,
    states_total: bool,
    states_next: bool,
    out_of_range: bool,
}

impl SpanFacts {
    /// The window of up to `length` `unit`s from `offset` over a view of
    /// `total`, clamped to the view (past the end it is empty at the end).
    pub(crate) fn window(unit: &'static str, offset: usize, length: usize, total: usize) -> Self {
        let offset = offset.min(total);
        Self {
            unit,
            offset,
            length: length.min(total - offset),
            total,
            states_total: true,
            states_next: true,
            out_of_range: false,
        }
    }

    /// The window of one part of a stream (a file inside a patch page):
    /// coordinates in that part alone, with no `nextOffset`, because the
    /// cursor that continues is the stream's, never the part's.
    pub(crate) fn local(unit: &'static str, offset: usize, length: usize, total: usize) -> Self {
        Self {
            states_next: false,
            ..Self::window(unit, offset, length, total)
        }
    }

    /// The same window without its `total`, for a view whose total the row
    /// already states (a line read's `totalLines`).
    pub(crate) fn without_total(mut self) -> Self {
        self.states_total = false;
        self
    }

    /// The same window, flagged as requested past the end (`outOfRange`).
    pub(crate) fn out_of_range(mut self, past_end: bool) -> Self {
        self.out_of_range = past_end;
        self
    }

    /// Whether units are left after the window.
    pub(crate) fn has_more(self) -> bool {
        self.offset + self.length < self.total
    }

    /// Where the next window starts, while units are left.
    pub(crate) fn next_offset(self) -> Option<usize> {
        self.has_more().then_some(self.offset + self.length)
    }

    /// The window block.
    pub(crate) fn to_value(self) -> Value {
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
mod tests {
    use super::*;

    #[test]
    fn known_and_open_pages_share_the_canonical_page_coordinate() {
        for (facts, expected_more) in [
            (PageFacts::counted(2, 10, 25), true),
            (PageFacts::open(2, None, false), false),
        ] {
            let output = facts.to_value();
            assert_eq!(output["page"], 2);
            assert_eq!(output["hasMore"], expected_more);
            assert!(output.get("currentPage").is_none());
        }
    }
    #[test]
    fn source_spans_preserve_coordinates_and_stop_at_the_end() {
        let first = SpanFacts::window("lines", 0, 10, 25);
        assert_eq!(first.next_offset(), Some(10));
        assert_eq!(first.to_value()["length"], 10);
        let last = SpanFacts::window("lines", 20, 10, 25);
        assert!(!last.has_more());
        assert_eq!(last.to_value()["length"], 5);
        assert_eq!(last.next_offset(), None);
        assert_eq!(
            SpanFacts::window("bytes", 900, 50, 100).to_value(),
            json!({"unit":"bytes","offset":100,"length":0,"total":100,"hasMore":false})
        );
    }
}
