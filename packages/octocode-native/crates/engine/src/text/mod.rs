pub(crate) mod diff_parser;
pub(crate) mod file_extension;
pub mod test_paths;
pub(crate) mod utf8_offsets;
pub(crate) mod yaml_utils;

/// The one extension helper, JS/TS extension set, and line index.
pub use file_extension::{JS_TS_EXTENSIONS, extension_of};
pub use utf8_offsets::LineIndex;
