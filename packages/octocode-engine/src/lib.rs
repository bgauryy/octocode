mod bindings;
mod lsp;
mod minify;
mod search;
mod security;
mod signatures;
mod structural;
mod text;
mod types;

pub(crate) use minify::{apply, comment_remover, config, minifier, strategies};
pub(crate) use search::{
    classify, fs_query, line_extractor, ripgrep_parser, ripgrep_pattern, ripgrep_search,
};
pub(crate) use text::{diff_parser, file_extension, line_diff, utf8_offsets, yaml_utils};

pub use bindings::*;
pub use lsp::client::NativeLspClient;
