//! `view:"compact"`: code without comments and spare whitespace, each kept
//! line under its source number. Only a grammar can say where a language's
//! comments and strings are, so without one compaction touches whitespace only
//! and says so. Markdown and data stay as they are.

use super::grammars::Spacing;
use super::layout::{Indent, Region, as_is, lay_out, whitespace_only};
use super::syntax;
use crate::text::extension_of;

/// Documents whose whitespace and `#` or `//` lines can all be content.
pub(crate) const STAYS_AS_IT_IS: &[&str] = &[
    "md", "markdown", "mdx", "yaml", "yml", "toml", "json", "jsonc", "json5",
];

/// One line of a compact view. `text` keeps its line ending, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeptLine {
    /// 1-based line of the source this text is from.
    pub line: usize,
    pub text: String,
}

/// Why a view holds the source's comments: it only lost blank lines and
/// trailing whitespace, inside any string too (the gap in its numbers shows
/// a dropped line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhitespaceOnly {
    /// No grammar for this file type in this build.
    NoGrammar,
    /// The grammar could not read most of the source.
    SyntaxErrors,
    /// The request was cancelled or ran out of time during the parse.
    Interrupted,
}

impl WhitespaceOnly {
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoGrammar => "whitespace-only-no-grammar",
            Self::SyntaxErrors => "whitespace-only-syntax-errors",
            Self::Interrupted => "whitespace-only-interrupted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactView {
    pub lines: Vec<KeptLine>,
    pub whitespace_only: Option<WhitespaceOnly>,
    /// The source is a document that stays as it is: `lines` are its lines,
    /// byte for byte, so the view is no smaller by design, not by content.
    pub as_is: bool,
}

/// Why a compact read of a document that stays as it is serves its content
/// ([`CompactView::as_is`]), as the readers report it.
pub const AS_IS_REASON: &str = "as-is-format";

/// How a source is laid out: where its comments and literals are, as byte
/// offsets. It holds none of the source's text, so it can be kept for the
/// later pages of the same source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexing(Layout);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Layout {
    AsIs,
    WhitespaceOnly(WhitespaceOnly),
    Grammar(Vec<Region>, Indent, Spacing),
}

impl Lexing {
    /// Whether a later page of the same source may reuse it: an interrupted
    /// parse is about that request only.
    #[must_use]
    pub fn reusable(&self) -> bool {
        self.0 != Layout::WhitespaceOnly(WhitespaceOnly::Interrupted)
    }

    /// The heap it holds, for a store's weight.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        match &self.0 {
            Layout::Grammar(regions, _, _) => std::mem::size_of_val(regions.as_slice()),
            _ => 0,
        }
    }
}

/// Find where `content`'s comments and literals are, `content` being a whole
/// source file at `file_path`. The parse ends when `stop` says its request is
/// over.
#[must_use]
pub fn lex(content: &str, file_path: &str, stop: &dyn Fn() -> bool) -> Lexing {
    let extension = extension_of(file_path, true, "txt");
    if STAYS_AS_IT_IS.contains(&extension.as_str()) {
        return Lexing(Layout::AsIs);
    }
    Lexing(match syntax::lex(content, &extension, stop) {
        Ok(lexed) => Layout::Grammar(lexed.regions, lexed.indent, lexed.spacing),
        Err(reason) => Layout::WhitespaceOnly(reason),
    })
}

/// The compact view of `content` as `lexing` (of this same text) lays it out.
#[must_use]
pub fn compact_view(content: &str, lexing: &Lexing) -> CompactView {
    match &lexing.0 {
        Layout::AsIs => CompactView {
            lines: as_is(content),
            whitespace_only: None,
            as_is: true,
        },
        Layout::WhitespaceOnly(reason) => CompactView {
            lines: whitespace_only(content),
            whitespace_only: Some(*reason),
            as_is: false,
        },
        Layout::Grammar(regions, indent, spacing) => CompactView {
            lines: lay_out(content, regions, *indent, *spacing),
            whitespace_only: None,
            as_is: false,
        },
    }
}

/// The compact view of `content`, a whole source file at `file_path`.
#[must_use]
pub fn apply_content_view_minification(content: &str, file_path: &str) -> CompactView {
    compact_view(content, &lex(content, file_path, &|| false))
}

#[cfg(test)]
pub(super) mod tests;
