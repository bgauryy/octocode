use super::super::cursor::{Cursor, PageScope};
use super::super::page_model::{ResponsePageOptions, ResponsePagination, restart_pagination};
use serde_json::{Value, json};

/// The pagination scope of a structured window.
const WINDOW_SCOPE: &str = "structuredContent";

pub(super) struct Page {
    pub(super) text: String,
    pub(super) pagination: Option<ResponsePagination>,
}

/// The window of the serialized envelope `text` that `options` name, with
/// its `structuredContent` pagination; a restart page carries its reason.
pub(super) fn paginate_window(text: &str, options: &ResponsePageOptions) -> Page {
    let Some(requested_length) = options.response_length else {
        return Page {
            text: text.into(),
            pagination: None,
        };
    };
    let units = text.encode_utf16().collect::<Vec<_>>();
    let total = units.len();
    let snapshot = super::super::cursor::window_snapshot(text);
    let length = requested_length.max(1);
    let requested_offset = options.response_offset.unwrap_or(0);
    let offset = requested_offset.min(total);
    let changed = options.response_snapshot.as_deref() != Some(&snapshot);
    let invalid = splits_pair(&units, offset);
    if requested_offset > 0 && (changed || invalid) {
        let reason = if invalid && !changed {
            "The requested offset splits a Unicode code point. Restart from the first page."
        } else if options.response_snapshot.is_some() {
            "The full response changed since the previous page. Discard earlier pages and restart from the first page."
        } else {
            "Later response pages require the previous page's cursor. Restart from the first page."
        };
        return Page {
            text: format!("# Response pagination restart required. {reason}\n"),
            pagination: Some(restart_pagination(
                WINDOW_SCOPE,
                page_number(&units, offset, length),
                offset,
                total_pages(&units, length),
                total,
                snapshot,
                options,
                changed,
                true,
            )),
        };
    }
    let end = choose_end(&units, offset, length);
    let has_more = end < total;
    let current = page_number(&units, offset, length);
    let pages = total_pages(&units, length);
    // `choose_end` never splits a UTF-16 surrogate pair, so this is lossless.
    Page {
        text: String::from_utf16_lossy(&units[offset..end]),
        pagination: Some(ResponsePagination {
            scope: WINDOW_SCOPE.into(),
            current_page: current,
            total_pages: pages,
            has_more,
            char_offset: offset,
            char_length: end - offset,
            total_chars: total,
            snapshot,
            expected_snapshot: None,
            changed: None,
            restart: None,
            next_char_offset: has_more.then_some(end),
            oversized: None,
            next: None,
        }),
    }
}

/// The next page's call: `{"responseSnapshot": cursor}`, one opaque token
/// naming the saved response and the page to serve, so a caller that copies
/// only that field reaches every page.
pub(super) fn build_continuation(
    tool: &str,
    request: &ResponsePageOptions,
    page: &ResponsePagination,
) -> Option<Value> {
    let tool = crate::id::ToolId::from_name(tool)?;
    let next_offset = page
        .next_char_offset
        .filter(|_| page.has_more || page.restart == Some(true))?;
    let cursor = cursor_at(request, page, &page.snapshot, next_offset)?;
    Some(
        crate::response::result::Continuation::input(tool, json!({"responseSnapshot": cursor}))
            .build(),
    )
}

/// The cursor of the page at `offset` of the content `snapshot` names, cut
/// as `request` cuts `page`. Without a saved response (a pager run outside
/// the runtime) the key is zero.
pub(super) fn cursor_at(
    request: &ResponsePageOptions,
    page: &ResponsePagination,
    snapshot: &str,
    offset: usize,
) -> Option<String> {
    let key = request.saved.as_ref().map_or([0; 32], |saved| saved.key);
    let scope = PageScope::of_page(&page.scope)?;
    let length = request.response_length?;
    Cursor::of_snapshot(key, snapshot, scope, length, offset).map(|cursor| cursor.encode())
}

fn splits_pair(units: &[u16], offset: usize) -> bool {
    offset > 0
        && offset < units.len()
        && (0xd800..=0xdbff).contains(&units[offset - 1])
        && (0xdc00..=0xdfff).contains(&units[offset])
}

fn choose_end(units: &[u16], start: usize, length: usize) -> usize {
    let raw = (start + length).min(units.len());
    if raw >= units.len() {
        return units.len();
    }
    let minimum = (length / 2).max(1);
    let mut boundary = None;
    for index in start..raw {
        if units[index] == b'\n' as u16 {
            boundary = Some(index + 1);
        }
        if index + 1 < raw && units[index] == b'\\' as u16 && units[index + 1] == b'n' as u16 {
            boundary = Some(index + 2);
        }
    }
    if let Some(value) = boundary.filter(|value| value - start >= minimum) {
        return value;
    }
    if splits_pair(units, raw) {
        if raw - start == 1 { raw + 1 } else { raw - 1 }
    } else {
        raw
    }
}

fn page_number(units: &[u16], offset: usize, length: usize) -> usize {
    let mut page = 1;
    let mut cursor = 0;
    while cursor < offset && cursor < units.len() {
        let next = choose_end(units, cursor, length);
        if next <= cursor {
            break;
        }
        cursor = next;
        page += 1;
    }
    if cursor == offset {
        page
    } else {
        offset / length + 1
    }
}

fn total_pages(units: &[u16], length: usize) -> usize {
    if units.is_empty() {
        return 1;
    }
    let mut pages = 0;
    let mut cursor = 0;
    while cursor < units.len() {
        let next = choose_end(units, cursor, length);
        if next <= cursor {
            return units.len().div_ceil(length).max(1);
        }
        cursor = next;
        pages += 1;
    }
    pages.max(1)
}
