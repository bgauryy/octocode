//! Leads that read: the top hit, every clipped value, the skipped binaries.
use crate::tools::local_search::local_search_query::Multiline;
use crate::tools::local_search::local_search_query::View;
use octocode_contract_types::RegexEngine;

use super::super::local_search_query;
use super::*;

impl Found<'_> {
    /// The calls that read every clipped value on the page whole
    /// (see [`numbered_calls`]).
    pub(super) fn expand_values_leads(&self) -> Vec<(String, Value)> {
        let query = self.query;
        let expansions = expand_values(
            self.paths,
            self.output_root,
            self.files,
            self.layout.context,
            local_search_query::multiline(query) != Multiline::Off,
        );
        numbered_calls("expandValues", &expansions)
    }

    /// The natural next step on a first page with hits: read the top hit in
    /// context. A shown declaration of the searched symbol reads that
    /// declaration whole when one block read holds it; a larger one (a big
    /// class) reads its hit in context, like any other hit. A partial page
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
        let text_file = |file: &&SearchFile| {
            !binary_cut
                .iter()
                .any(|cut| std::path::Path::new(cut).ends_with(&file.path))
        };
        let declared = self.definition.and_then(|definition| {
            self.files
                .iter()
                .filter(text_file)
                .find(|file| {
                    self.output_root.join(&file.path).to_string_lossy()
                        == definition.source.as_str()
                })
                .zip(Some(definition))
        });
        match declared {
            Some((file, definition))
                if (definition.end + 1).saturating_sub(definition.start) as usize
                    <= crate::tools::source_view::BLOCK_MAX_LINES =>
            {
                Some(declaration_read(
                    query,
                    self.root,
                    file,
                    definition.start,
                    definition.end,
                ))
            }
            Some((file, _)) => read_handoff(query, self.root, file),
            None => self
                .files
                .iter()
                .find(text_file)
                .and_then(|top| read_handoff(query, self.root, top)),
        }
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
    /// a coverage gap: disclosed once, on the first page, as a count by
    /// extension with a listing.
    /// The listing also covers binary-cut files too many to name in their
    /// own warning ([`binary_file_list`]).
    pub(in crate::tools::local_search) fn skipped_binaries(&self) -> Option<(String, Value)> {
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
        let listing =
            leading_binary_listing(self.query, &merge_binary_groups(skipped, &cut_groups));
        let warning = if skipped.is_empty() {
            // The cut files' own warning carries the count.
            String::new()
        } else {
            leading_binary_warning(skipped)
        };
        Some((warning, listing))
    }
}

/// Executable calls from `reads` (one-row calls, see [`expand_values`]):
/// each run of one tool's rows in calls of as many rows as that tool
/// accepts, the first named `name` and the rest `name2`, `name3`, …, so
/// every read is one call away. A default page is cut so its reads fit one
/// call (see `layout::stream::stream_pages`); a caller-sized page may need more.
fn numbered_calls(name: &str, reads: &[Value]) -> Vec<(String, Value)> {
    let mut calls = Vec::new();
    for run in reads.chunk_by(|a, b| a["tool"] == b["tool"]) {
        let Some(tool) = run[0]["tool"].as_str().and_then(ToolId::from_name) else {
            continue;
        };
        let rows = run
            .iter()
            .filter_map(crate::response::result::continuation_row)
            .cloned()
            .collect::<Vec<_>>();
        for chunk in rows.chunks(crate::response::result::rows_per_call(tool)) {
            let key = crate::id::numbered(name, calls.len());
            let call = Continuation::related_input(tool, json!({ "queries": chunk })).build();
            calls.push((key, call));
        }
    }
    calls
}
