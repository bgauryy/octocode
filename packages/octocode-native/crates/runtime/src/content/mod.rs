mod file_type;
mod markdown_outline;

pub use file_type::{FileType, classify_file_type};
pub(crate) use markdown_outline::markdown_heading_outline;
pub use octocode_engine::text::test_paths::is_test_path;

/// `value[start..end]` in UTF-16 code units (the unit of provider match
/// indices); `None` when out of bounds or splitting a surrogate pair.
pub(crate) fn utf16_slice(value: &str, start: usize, end: usize) -> Option<String> {
    let text = value.encode_utf16().collect::<Vec<_>>();
    String::from_utf16(text.get(start..end)?).ok()
}
