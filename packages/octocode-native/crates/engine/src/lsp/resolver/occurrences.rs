//! Code occurrence ranges and their reusable parsed document.
use super::syntax::SymbolCandidate;
use crate::lsp::types::{LspExactPosition, LspRange};
use crate::signatures::extractor::whole_file_deadline;
use crate::text::LineIndex;

/// Where a name occurs as code in one parsed source.
pub struct CodeOccurrences {
    /// Zero-based lines, see [`super::code_occurrence_lines`].
    pub lines: Vec<usize>,
    ranges: Vec<LspRange>,
    tree: tree_sitter::Tree,
    language: &'static tree_sitter::Language,
}

impl CodeOccurrences {
    pub(super) fn from_candidates(
        candidates: Vec<SymbolCandidate>,
        index: &LineIndex,
        content: &str,
        tree: tree_sitter::Tree,
        language: &'static tree_sitter::Language,
    ) -> Self {
        let mut candidates = candidates
            .into_iter()
            .filter(|candidate| candidate.is_exact && !candidate.is_literal)
            .collect::<Vec<_>>();
        // A wrapper and its name can cover one identity (a Rust lifetime
        // and its identifier). Keep the complete spelling once.
        candidates.sort_unstable_by_key(|candidate| {
            (
                candidate.source.start,
                std::cmp::Reverse(candidate.source.end),
            )
        });
        candidates.dedup_by(|next, previous| {
            previous.source.start <= next.source.start && next.source.end <= previous.source.end
        });
        let position = |byte| {
            let (line, character) =
                index.byte_to_position(content, crate::num::saturating_u32(byte));
            LspExactPosition { line, character }
        };
        let ranges = candidates
            .into_iter()
            .map(|candidate| LspRange {
                start: position(candidate.source.start),
                end: position(candidate.source.end),
            })
            .collect::<Vec<_>>();
        let mut lines = ranges
            .iter()
            .map(|range| range.start.line as usize)
            .collect::<Vec<_>>();
        lines.dedup();
        Self {
            lines,
            ranges,
            tree,
            language,
        }
    }

    /// Unique UTF-16 code-name ranges, preserving raw and quoted declaration
    /// spelling while excluding comments and unrelated value literals.
    #[must_use]
    pub fn ranges(&self) -> &[LspRange] {
        &self.ranges
    }

    /// Where `name` is declared and called in this source, reusing its parse.
    /// `None` when only the whole-file facts can establish the sites.
    #[must_use]
    pub fn sites(
        &self,
        file_path: &str,
        content: &str,
        name: &str,
    ) -> Option<crate::signatures::graph_facts::NamedSites> {
        use crate::signatures::{extractor, graph_facts, graph_facts_grammar};
        let (entry, grammar) = graph_facts_grammar(content, file_path)?;
        let own_parse;
        let tree = if std::ptr::eq(&entry.language, self.language) {
            &self.tree
        } else {
            own_parse = extractor::parse_before(content, &entry.language, whole_file_deadline())?;
            &own_parse
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            graph_facts::named_sites_from_tree(content, file_path, &grammar, tree, name)
        }))
        .unwrap_or(None)
    }
}
