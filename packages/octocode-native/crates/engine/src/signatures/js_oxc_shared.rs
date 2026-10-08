use oxc_ast::ast::{ModuleExportName, PropertyKey};
use oxc_span::Span;

use crate::graph::{GraphPosition, GraphRange};

/// Maps oxc byte spans to graph `(line, character)` positions, where
/// `character` counts UTF-16 code units from the line start (the LSP wire
/// convention): the shared [`LineIndex`] under the tree-sitter rule, so oxc
/// and tree-sitter positions agree.
///
/// [`LineIndex`]: crate::text::LineIndex
pub(super) struct SpanPositions<'a> {
    content: &'a str,
    index: crate::text::LineIndex,
}

impl<'a> SpanPositions<'a> {
    pub(super) fn new(content: &'a str) -> Self {
        Self {
            content,
            index: crate::text::LineIndex::tree_sitter(content),
        }
    }

    pub(super) fn position(&self, byte_offset: u32) -> GraphPosition {
        let (line, character) = self.index.byte_to_position(self.content, byte_offset);
        GraphPosition { line, character }
    }

    pub(super) fn range(&self, span: Span) -> GraphRange {
        GraphRange {
            start: self.position(span.start),
            end: self.position(span.end),
        }
    }

    /// Convert an LSP `(line, character)` (0-based, UTF-16) to a byte offset.
    pub(super) fn byte_offset(&self, line: u32, character: u32) -> u32 {
        self.index.position_to_byte(self.content, line, character)
    }
}

pub(super) fn module_export_name(name: &ModuleExportName) -> Option<String> {
    match name {
        ModuleExportName::IdentifierName(id) => Some(id.name.as_str().to_string()),
        ModuleExportName::IdentifierReference(id) => Some(id.name.as_str().to_string()),
        ModuleExportName::StringLiteral(s) => Some(s.value.as_str().to_string()),
    }
}

pub(super) fn property_key_name(key: &PropertyKey) -> Option<(String, Span)> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some((id.name.as_str().to_string(), id.span)),
        PropertyKey::PrivateIdentifier(p) => Some((format!("#{}", p.name.as_str()), p.span)),
        PropertyKey::StringLiteral(s) => Some((s.value.as_str().to_string(), s.span)),
        _ => None,
    }
}
