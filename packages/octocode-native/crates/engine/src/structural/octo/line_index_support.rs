use std::collections::HashMap;

use tree_sitter::Node;

use crate::structural::types::{MetavarRange, StructuralMatch};
use crate::text::LineIndex;

use super::matching::{RawRange, node_text};
use super::source_context::SourceContext;

/// Converts raw tree-sitter capture positions into `MetavarRange`s (1-based
/// line, char column), pairing each range with its captured text by index.
fn build_metavar_ranges(
    source: &SourceContext,
    content: &str,
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
                .map(|(i, (sr, sc, er, ec, start, end, tree))| MetavarRange {
                    source_span: source.range(tree, start, end),
                    source_origin: None,
                    text: texts.and_then(|t| t.get(i)).cloned().unwrap_or_default(),
                    line: sr + 1,
                    column: line_index.row_col_to_utf16_column(content, sr, sc),
                    end_line: er + 1,
                    end_column: line_index.row_col_to_utf16_column(content, er, ec),
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
    let line_index = LineIndex::tree_sitter(content);
    to_structural_match_with_index(node, content, &line_index, metavars, metavar_ranges_raw)
}

pub(super) fn to_structural_match_with_index(
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let source = SourceContext::root(node, content);
    to_structural_match_with_context(
        &source,
        node,
        content,
        line_index,
        metavars,
        metavar_ranges_raw,
    )
}

pub(super) fn to_structural_match_with_context(
    source: &SourceContext,
    node: Node<'_>,
    content: &str,
    line_index: &LineIndex,
    metavars: HashMap<String, Vec<String>>,
    metavar_ranges_raw: HashMap<String, Vec<RawRange>>,
) -> StructuralMatch {
    let start = node.start_position();
    let end = node.end_position();
    let metavar_ranges =
        build_metavar_ranges(source, content, line_index, &metavars, metavar_ranges_raw);
    let source_span = source.span(node);
    let header = block_header(node, content);
    let header_span = header
        .as_ref()
        .map(|header| crate::text::analysis::ObservedSpan {
            unit: source_span.unit,
            range: crate::text::analysis::ByteRange::new(
                source_span.start_byte,
                source_span.start_byte + header.len() as u32,
            ),
        });
    StructuralMatch {
        source_span,
        header_span,
        source_templates: std::sync::Arc::clone(&source.templates),
        source_origin: None,
        header_origin: None,
        source_descriptors: None,
        start_line: (start.row as u32) + 1,
        end_line: (end.row as u32) + 1,
        start_col: line_index.row_col_to_utf16_column(
            content,
            start.row as u32,
            start.column as u32,
        ),
        end_col: line_index.row_col_to_utf16_column(content, end.row as u32, end.column as u32),
        text: node_text(node, content).to_owned(),
        metavars,
        metavar_ranges,
        header,
    }
}

/// Source before a multi-line node's `body` child (the field name tree-sitter
/// grammars share for function, class, impl, and loop bodies).
fn block_header(node: Node<'_>, content: &str) -> Option<String> {
    if node.end_position().row == node.start_position().row {
        return None;
    }
    let body = node.child_by_field_name("body")?;
    content
        .get(node.start_byte()..body.start_byte())
        .map(str::trim_end)
        .filter(|header| !header.is_empty())
        .map(str::to_owned)
}
