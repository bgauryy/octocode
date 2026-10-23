use super::{LineIndex, resolve_position_in_file_content};
use crate::lsp::types::LspFuzzyPosition;

fn fuzzy(name: &str, line_hint: Option<u32>) -> LspFuzzyPosition {
    LspFuzzyPosition {
        symbol_name: name.to_owned(),
        line_hint,
        order_hint: None,
    }
}

mod anchors;
mod declarations;
mod lines_and_budget;
