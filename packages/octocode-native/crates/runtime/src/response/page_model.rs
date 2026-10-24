//! Internal immutable response-page plan. Only responseChars is caller input.
use super::cursor::Cursor;
use serde_json::Value;

#[derive(Clone, Debug, Default, serde::Deserialize)]
pub(crate) struct ResponsePageOptions {
    #[serde(rename = "responseChars")]
    pub(crate) response_chars: Option<usize>,
    #[serde(skip)]
    pub(crate) response_offset: Option<usize>,
    #[serde(skip)]
    pub(crate) response_snapshot: Option<String>,
    #[serde(skip)]
    pub(crate) paging: bool,
    #[serde(skip)]
    pub(crate) saved: Option<SavedResponse>,
}

#[derive(Clone, Debug)]
pub(crate) struct SavedResponse {
    pub key: [u8; 32],
    pub expires_at: String,
    pub replayed: bool,
}

impl ResponsePageOptions {
    pub(crate) fn open(&mut self, cursor: &Cursor) {
        self.response_snapshot = Some(cursor.content_snapshot());
        self.response_chars = Some(cursor.length);
        self.response_offset = Some(cursor.offset);
        self.paging = true;
    }
    pub(crate) fn rows_scope(&self) -> bool {
        self.paging
    }
    pub(crate) fn scoped(&self) -> bool {
        self.paging
    }
    pub(crate) fn pages(&self, structured: &Value, auto_page: usize) -> bool {
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
    pub(crate) page: usize,
    pub(crate) total_pages: usize,
    pub(crate) has_more: bool,
    pub(crate) returned_chars: usize,
    pub(crate) total_chars: usize,
    pub(crate) snapshot: String,
    pub(crate) next_offset: Option<usize>,
    pub(crate) oversized: Option<bool>,
    pub(crate) next: Option<Value>,
}
impl ResponsePagination {
    pub(crate) fn should_emit(&self) -> bool {
        self.has_more || self.next.is_some() || self.oversized == Some(true)
    }
}
