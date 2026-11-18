//! Internal immutable response-page plan. Only responseChars is caller input.
use super::cursor::Cursor;
use serde_json::Value;

#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct ResponsePageOptions {
    #[serde(rename = "responseChars")]
    pub response_chars: Option<usize>,
    #[serde(skip)]
    pub response_offset: Option<usize>,
    #[serde(skip)]
    pub response_snapshot: Option<String>,
    #[serde(skip)]
    pub paging: bool,
    #[serde(skip)]
    pub saved: Option<SavedResponse>,
    /// The identity of the build that serves the call, which every content
    /// snapshot and cursor of its pages binds to: the runtime sets it from
    /// its build identity, so a page is never cut against another build's.
    #[serde(skip)]
    pub build_identity: &'static str,
}

#[derive(Clone, Debug)]
pub struct SavedResponse {
    pub key: [u8; 32],
    pub expires_at: String,
    pub replayed: bool,
}

impl ResponsePageOptions {
    pub fn open(&mut self, cursor: &Cursor) {
        self.response_snapshot = Some(cursor.content_snapshot(self.build_identity));
        self.response_chars = Some(cursor.length);
        self.response_offset = Some(cursor.offset);
        self.paging = true;
    }
    pub fn pages(&self, structured: &Value, auto_page: usize) -> bool {
        self.paging
            || self
                .response_chars
                .or((auto_page != 0).then_some(auto_page))
                .is_some_and(|budget| {
                    octocode_engine::json_size::utf16_units(structured).unwrap_or(0) > budget
                })
    }
    pub(crate) fn auto_paginate(&mut self, structured: &Value, auto_page: usize) {
        if self.paging {
            return;
        }
        if self.pages(structured, auto_page) {
            self.response_chars = self.response_chars.or(Some(auto_page));
            self.paging = true;
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResponsePagination {
    /// How many pages the plan cut (the plan tests walk them by it).
    #[cfg(test)]
    pub(crate) total_pages: usize,
    pub(crate) has_more: bool,
    /// The page's chars as measured against the budget (the plan tests
    /// check page widths by it).
    #[cfg(test)]
    pub(crate) returned_chars: usize,
    pub(crate) snapshot: String,
    pub(crate) next_offset: Option<usize>,
    pub(crate) next: Option<Value>,
}
impl ResponsePagination {
    /// A fresh response states its block only while a walk remains. A
    /// complete one-page response is no page of a split, so an oversized
    /// indivisible value there is not a reason to state one: a block with
    /// `hasMore:false` would read as the end of a walk that never began. Its
    /// size is stated by the envelope's top-level `oversized` instead.
    pub(crate) fn should_emit(&self) -> bool {
        self.has_more || self.next.is_some()
    }
}
