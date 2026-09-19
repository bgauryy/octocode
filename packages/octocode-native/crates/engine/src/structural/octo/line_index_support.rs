use std::collections::HashMap;

use tree_sitter::Node;

use crate::structural::types::{MetavarRange, StructuralMatch};

use super::matching::{node_text, RawRange};

/// Thin wrapper over the shared `text::utf8_offsets::LineIndex` — see that
/// type for the actual line-start/UTF-16 counting logic. Keeps this module's
/// 1-based-line, tree-sitter-point-shaped call sites unchanged.
pub(super) struct LineIndex<'a>(crate::text::utf8_offsets::LineIndex<'a>);

impl<'a> LineIndex<'a> {
    pub(super) fn new(content: &'a str) -> Self {
        Self(crate::text::utf8_offsets::LineIndex::new(content))
    }

    fn byte_to_line_col(&self, byte: usize) -> (usize, usize) {
        let (row, column) = self.0.byte_to_position(byte as u32);
        (row as usize + 1, column as usize)
    }

    /// Convert a tree-sitter byte column to an LSP-compatible **UTF-16 code-unit**
    /// column. This is the unit `lspSearch` uses, the JS resolver emits
    /// (`resolver::byte_offset_to_utf16`), and the signatures layer reports
    /// (`char::len_utf16`). Counting Unicode scalar values (`chars().count()`)
    /// instead would disagree with every other layer on any line containing a
    /// non-BMP character (e.g. an emoji is one code point but two UTF-16 units).
    pub(super) fn point_column_to_char_column(&self, row: usize, byte_column: usize) -> usize {
        self.0
            .row_col_to_utf16_column(row as u32, byte_column as u32) as usize
    }
}

/// Converts raw tree-sitter capture positions into `MetavarRange`s (1-based
/// line, char column), pairing each range with its captured text by index.
fn build_metavar_ranges(
    line_index: &LineIndex,
    values: &HashMap<String, Vec<String>>,
    raw: HashMap<String, Vec<RawRange>>,
) -> HashMap<String, Vec<MetavarRange>> {
    raw.into_iter()
        .map(|(name, ranges)| {
            let texts = values.get(&name);
            let mapped = ranges
                .into_iter()
                .enumerate()
                .map(|(i, (sr, sc, er, ec))| MetavarRange {
                    text: texts.and_then(|t| t.get(i)).cloned().unwrap_or_default(),
                    line: sr + 1,
                    column: line_index.point_column_to_char_column(sr as usize, sc as usize) as u32,
                    end_line: er + 1,
                    end_column: line_index.point_column_to_char_column(er as usize, ec as usize)
                        as u32,
                })
                .collect();
            (name, mapped)
        })
        .collect()
}

pub(super) fn to_structural_match(
    node: Node<'_>,
    content: &str,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let line_index = LineIndex::new(content);
    to_structural_match_with_index(node, content, &line_index, metavars, metavar_ranges_raw)
}

pub(super) fn to_structural_match_with_index(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let start = node.start_position();
    let end = node.end_position();
    let metavar_ranges = build_metavar_ranges(line_index, &metavars, metavar_ranges_raw);
    StructuralMatch {
        start_line: (start.row as u32) + 1,
        end_line: (end.row as u32) + 1,
        start_col: line_index.point_column_to_char_column(start.row, start.column) as u32,
        end_col: line_index.point_column_to_char_column(end.row, end.column) as u32,
        text: node_text(node, content).to_owned(),
        metavars,
        metavar_ranges,
    }
}

pub(super) fn structural_match_from_byte_range_with_index(
    content: &str,
    line_index: &LineIndex,
    start_byte: usize,
    end_byte: usize,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let (start_line, start_col) = line_index.byte_to_line_col(start_byte);
    let (end_line, end_col) = line_index.byte_to_line_col(end_byte);
    let metavar_ranges = build_metavar_ranges(line_index, &metavars, metavar_ranges_raw);
    StructuralMatch {
        start_line: start_line as u32,
        end_line: end_line as u32,
        start_col: start_col as u32,
        end_col: end_col as u32,
        text: content
            .get(start_byte..end_byte)
            .unwrap_or_default()
            .to_owned(),
        metavars,
        metavar_ranges,
    }
}
