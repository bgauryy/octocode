use tree_sitter::Language;

use crate::signatures::languages;
use crate::text::extension_of;

/// The grammar of a file with an LSP language id, read from the single
/// language registry (`signatures::languages`).
pub(crate) struct GrammarSpec {
    pub(crate) language_id: &'static str,
    language: &'static Language,
}

impl GrammarSpec {
    pub(crate) fn language(&self) -> &'static Language {
        self.language
    }

    pub(crate) fn parse_before(
        &self,
        content: &str,
        deadline: std::time::Instant,
    ) -> Option<tree_sitter::Tree> {
        crate::signatures::extractor::parse_before(content, self.language, deadline)
    }
}

/// The grammar that also parses `file_path`'s extension without owning it
/// (C++ for a `.h` header): the second reading when the owner's parse has
/// errors. `None` when no other entry parses it or it carries no language id.
pub(crate) fn sibling_grammar_for_file(file_path: &str) -> Option<GrammarSpec> {
    let extension = extension_of(file_path, true, "");
    let entry = languages::all_entries().iter().find(|entry| {
        entry
            .also_parses
            .iter()
            .any(|ext| ext.eq_ignore_ascii_case(&extension))
    })?;
    Some(GrammarSpec {
        language_id: entry.language_id?,
        language: &entry.language,
    })
}

/// The registry grammar for `file_path`'s extension; `None` when no entry
/// owns the extension or the entry carries no LSP language id.
pub(crate) fn grammar_for_file(file_path: &str) -> Option<GrammarSpec> {
    let entry = languages::find_entry(&extension_of(file_path, true, ""))?;
    Some(GrammarSpec {
        language_id: entry.language_id?,
        language: &entry.language,
    })
}

#[cfg(test)]
mod tests;
