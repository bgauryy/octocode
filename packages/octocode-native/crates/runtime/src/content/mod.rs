mod file_type;
mod markdown_outline;

pub use file_type::{FileType, classify_file_type, is_test_path};
pub(crate) use markdown_outline::markdown_heading_outline;
