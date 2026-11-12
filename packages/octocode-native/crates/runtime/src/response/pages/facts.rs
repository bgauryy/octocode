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
pub(crate) struct PageFacts {
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
    pub(crate) fn open(offset: usize, length: usize, has_more: bool) -> Self {
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
    pub(crate) fn counted(offset: usize, length: usize, total: usize) -> Self {
        Self {
            has_more: offset.saturating_add(length) < total,
            ..Self::open(offset, length, false).with_total(total)
        }
    }

    /// Count page `page` (1-based; 0 reads as the first) of `size` items each
    /// over a list of `total`. A page past the end is empty at the end and
    /// flagged `outOfRange`; page 1 of an empty list is the empty list itself.
    pub(crate) fn nth(page: usize, size: usize, total: usize) -> Self {
        let window = crate::response::page_windows::count_window(page, size, total);
        Self::counted(window.start, window.len(), total)
            .out_of_range(page.max(1) > crate::response::page_windows::count_pages(total, size))
    }

    /// The start of the count page `page` (1-based; 0 reads as the first) of
    /// `size` items each, for a list whose size is unknown.
    pub(crate) fn start_of(page: usize, size: usize) -> usize {
        page.max(1).saturating_sub(1).saturating_mul(size.max(1))
    }

    /// A section before its first page: none of its `total` items shown,
    /// every one still to read.
    pub(crate) fn unshown(total: usize) -> Self {
        Self::counted(0, 0, total)
    }

    /// The same page counting `unit` (`files`, `matches`) instead of items.
    pub(crate) fn with_unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }

    /// The same page with the size of its whole list, exact or a floor (the
    /// caller's `countScope` says which).
    pub(crate) fn with_total(mut self, total: usize) -> Self {
        self.total = Some(total);
        self
    }

    /// The same page, flagged as requested past the end (`outOfRange`).
    pub(crate) fn out_of_range(mut self, past_end: bool) -> Self {
        self.out_of_range = past_end;
        self
    }

    /// Whether the page lies past the end of its list.
    pub(crate) fn is_past_end(self) -> bool {
        self.out_of_range
    }

    /// The `pagination` block.
    pub(crate) fn to_value(self) -> Value {
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
    fn list_pages_state_offset_length_and_total() {
        assert_eq!(
            PageFacts::nth(2, 10, 25).to_value(),
            json!({"unit":"items","offset":10,"length":10,"total":25,"hasMore":true})
        );
        assert_eq!(
            PageFacts::nth(3, 10, 25).with_unit("files").to_value(),
            json!({"unit":"files","offset":20,"length":5,"total":25,"hasMore":false})
        );
        assert_eq!(
            PageFacts::open(10, 10, false).to_value(),
            json!({"unit":"items","offset":10,"length":10,"hasMore":false})
        );
        assert_eq!(
            PageFacts::unshown(4).to_value(),
            json!({"unit":"items","offset":0,"length":0,"total":4,"hasMore":true})
        );
    }

    #[test]
    fn a_count_page_past_the_end_is_empty_at_the_end_and_flagged() {
        let past = PageFacts::nth(4, 10, 25);
        assert!(past.is_past_end());
        assert_eq!(past.to_value()["offset"], 25);
        assert_eq!(past.to_value()["length"], 0);
        assert!(!PageFacts::nth(1, 10, 0).is_past_end(), "page 1 of nothing");
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
