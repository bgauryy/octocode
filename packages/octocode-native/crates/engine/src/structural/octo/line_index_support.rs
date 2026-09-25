use std::collections::HashMap;

use tree_sitter::Node;

use crate::structural::types::{MetavarRange, StructuralMatch};
use crate::text::utf8_offsets::LineIndex;

use super::matching::{RawRange, node_text};

/// Converts raw tree-sitter capture positions into `MetavarRange`s (1-based
/// line, char column), pairing each range with its captured text by index.
fn build_metavar_ranges(
    line_index: &LineIndex,
    values: &HashMap<String, Vec<String>>,
    raw: HashMap<String, Vec<RawRange>>,
) -> std::collections::BTreeMap<String, Vec<MetavarRange>> {
    raw.into_iter()
        .map(|(name, ranges)| {
            let texts = values.get(&name);
            let mapped = ranges
                .into_iter()
                .enumerate()
                .map(|(i, (sr, sc, er, ec))| MetavarRange {
                    text: texts.and_then(|t| t.get(i)).cloned().unwrap_or_default(),
                    line: sr + 1,
                    column: line_index.row_col_to_utf16_column(sr, sc),
                    end_line: er + 1,
                    end_column: line_index.row_col_to_utf16_column(er, ec),
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
        start_col: line_index.row_col_to_utf16_column(start.row as u32, start.column as u32),
        end_col: line_index.row_col_to_utf16_column(end.row as u32, end.column as u32),
        text: node_text(node, content).to_owned(),
        metavars,
        metavar_ranges,
    }
}
