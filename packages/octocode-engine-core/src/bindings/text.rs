use crate::types::{
    ExtractMatchingLinesOptions, ExtractMatchingLinesResult, FilterPatchOptions,
    SliceContentOptions, SliceContentResult,
};
use napi::Result;
use napi_derive::napi;

/// Number of UTF-8 bytes up to (not including) the `char_index`-th JavaScript
/// UTF-16 code unit in `content`. Zero-allocation — no `Buffer.from()` needed.
#[napi(js_name = "charToByteOffset")]
pub fn char_to_byte_offset(content: String, char_index: u32) -> u32 {
    crate::portable::char_to_byte_offset(&content, char_index as usize) as u32
}

/// JavaScript UTF-16 code-unit offset for `byte_offset` bytes into `content`.
/// Zero-allocation — no `Buffer.from()` needed.
#[napi(js_name = "byteToCharOffset")]
pub fn byte_to_char_offset(content: String, byte_offset: u32) -> u32 {
    crate::portable::byte_to_char_offset(&content, byte_offset as usize) as u32
}

/// Extract a byte-range substring from `content`.
#[napi(js_name = "byteSliceContent")]
pub fn byte_slice_content(content: String, byte_start: u32, byte_end: u32) -> String {
    crate::portable::byte_slice_content(&content, byte_start as usize, byte_end as usize)
}

/// Paginate `content` by char offset + length, with optional line-boundary
/// snapping. Replaces both the char-mode conversion block in `applyPagination`
/// and the dead-code `sliceByCharRespectLines` (0 callers confirmed by LSP).
#[napi(js_name = "sliceContent")]
pub fn slice_content(
    content: String,
    char_offset: u32,
    char_length: u32,
    options: Option<SliceContentOptions>,
) -> SliceContentResult {
    crate::portable::slice_content(
        &content,
        char_offset as usize,
        char_length as usize,
        options,
    )
}

/// Search `content` line-by-line for `pattern` (literal or regex), returning
/// matched lines with context windows and omission markers.
///
/// Replaces `extractMatchingLines` (contentExtractor.ts) which performed 2–3
/// full `forEach` scans with per-line `toLowerCase` + `RegExp.test`.
#[napi(js_name = "extractMatchingLines")]
pub fn extract_matching_lines(
    content: String,
    pattern: String,
    options: Option<ExtractMatchingLinesOptions>,
) -> Result<ExtractMatchingLinesResult> {
    Ok(crate::portable::extract_matching_lines(
        &content, &pattern, options,
    )?)
}

/// Filter and optionally trim a unified diff patch.
///
/// Replaces `filterPatch` + `trimDiffContext` from `utils/parsers/diff.ts` which
/// called `patch.split('\n')` independently in both functions. This combines
/// both operations in a single pass.
#[napi(js_name = "filterPatch")]
pub fn filter_patch(patch: String, options: Option<FilterPatchOptions>) -> String {
    crate::portable::filter_patch(&patch, options)
}
