//! Leads that read the top hit or list skipped binaries.
use crate::tools::local_search::local_search_query::View;
use octocode_contract_types::RegexEngine;

use super::super::local_search_query;
use super::*;

/// A first-page binary disclosure, with a listing only for unread cut text.
pub(in crate::tools::local_search) struct BinaryDisclosure {
    pub(in crate::tools::local_search) listing: Option<Value>,
}

impl Found<'_> {
    /// The natural next step on a first page with hits: read the top hit in
    /// context. A shown declaration of the searched symbol reads that
    /// declaration whole (localFetch pages a long one). A partial page
    /// (a binary cut, an unreadable path) still reads its shown hits; a file
    /// cut at a NUL byte is never the read (localFetch rejects binaries).
    pub(in crate::tools::local_search) fn read_lead(&self) -> Option<Value> {
        let query = self.query;
        let readable = self.first_page()
            && !self.layout.list
            && local_search_query::view(query) != View::MatchOnly
            && self.layout.context.is_empty()
            && local_search_query::regex_mode(query) != RegexEngine::Pcre2
            && local_search_query::invert_match(query) != Some(true);
        if !readable {
            return None;
        }
        let binary_cut = self.scanned.binary_files.as_deref().unwrap_or_default();
        // Nor is a source refused whatever part of it is asked.
        let text_file = |file: &&SearchFile| {
            !binary_cut
                .iter()
                .any(|cut| std::path::Path::new(cut).ends_with(&file.path))
                && !crate::tools::source_view::prepare::every_read_refused(
                    &source_of(self.root, file),
                    self.limits,
                )
        };
        let declared = self.definition.and_then(|definition| {
            self.files
                .iter()
                .find(|file| {
                    self.output_root.join(&file.path).to_string_lossy()
                        == definition.source.as_str()
                })
                .filter(text_file)
                .zip(Some(definition))
        });
        match declared {
            Some((file, definition)) => {
                declaration_read(query, self.root, file, definition.start, definition.end)
            }
            None => self
                .files
                .iter()
                .find(text_file)
                .and_then(|top| read_handoff(query, self.root, top)),
        }
    }

    /// The page's hits that show a window of a line too long for a row, not
    /// the line whole: one warning counting them, naming the first, and the
    /// localFetch read that serves the whole line (the read lead, on a page
    /// that offers one).
    pub(in crate::tools::local_search) fn windowed_hits(&self) -> Option<String> {
        let mut windows = self.files.iter().flat_map(|file| {
            file.matches
                .iter()
                .flatten()
                .filter(|hit| hit.window_start.is_some())
                .map(move |hit| (file, hit))
        });
        let (file, first) = windows.next()?;
        let count = 1 + windows.count();
        Some(format!(
            "windowedHits: {count} {} a window of a line too long for a row (first {}:{}); localFetch with the same matchString reads the whole line.",
            if count == 1 { "hit shows" } else { "hits show" },
            joined_path(self.query, self.root, file),
            first.line
        ))
    }

    /// Files skipped as binary from their leading bytes, when any.
    pub(in crate::tools::local_search) fn leading_binaries(&self) -> Option<u32> {
        let total: u32 = self
            .scanned
            .skipped_binary_extensions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|group| group.count)
            .sum();
        (total > 0).then_some(total)
    }

    /// Files binary from their leading bytes are outside a text search, not
    /// a coverage gap: disclosed once, on the first page, as a count
    /// (`binarySkipped`). `Some` when there are any, with the listing of the
    /// binary-cut files too many to name in their own warning
    /// ([`binary_file_list`]), on any row: their text past the NUL is unread.
    pub(in crate::tools::local_search) fn skipped_binaries(&self) -> Option<BinaryDisclosure> {
        if !self.first_page() || self.root.is_file() {
            return None;
        }
        let skipped = self
            .scanned
            .skipped_binary_extensions
            .as_deref()
            .unwrap_or_default();
        let cut = self.scanned.binary_files.as_deref().unwrap_or_default();
        let cut_groups = if cut.len() > MAX_NAMED_BINARY_FILES {
            binary_groups(cut)
        } else {
            Vec::new()
        };
        if skipped.is_empty() && cut_groups.is_empty() {
            return None;
        }
        Some(BinaryDisclosure {
            listing: (!cut_groups.is_empty())
                .then(|| leading_binary_listing(self.query, &cut_groups))
                .flatten(),
        })
    }
}
