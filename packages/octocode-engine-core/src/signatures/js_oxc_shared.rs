use oxc_ast::ast::{ModuleExportName, PropertyKey};
use oxc_span::Span;
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct Position {
    pub(super) line: u32,
    pub(super) character: u32,
}

#[derive(Serialize)]
pub(super) struct Range {
    pub(super) start: Position,
    pub(super) end: Position,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GraphCall {
    pub(super) id: String,
    pub(super) caller: String,
    pub(super) callee: String,
    pub(super) line: u32,
    pub(super) range: Range,
    pub(super) kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GraphCommonJsLoad {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) specifier: Option<String>,
    pub(super) line: u32,
    pub(super) kind: &'static str,
    pub(super) binding: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reason: Option<&'static str>,
}

/// Maps byte offsets to LSP `(line, character)` positions, where `character`
/// counts UTF-16 code units from the line start (the LSP wire convention).
pub(super) struct LineIndex<'a>(crate::text::utf8_offsets::LineIndex<'a>);

impl<'a> LineIndex<'a> {
    pub(super) fn new(content: &'a str) -> Self {
        Self(crate::text::utf8_offsets::LineIndex::new(content))
    }

    pub(super) fn position(&self, byte_offset: u32) -> Position {
        let (line, character) = self.0.byte_to_position(byte_offset);
        Position { line, character }
    }

    pub(super) fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.end),
        }
    }

    /// Convert an LSP `(line, character)` (0-based, UTF-16) to a byte offset.
    pub(super) fn byte_offset(&self, line: u32, character: u32) -> u32 {
        self.0.position_to_byte(line, character)
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
