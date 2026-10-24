//! What a page cut charges: file entries, rows, the reads that expand
//! clipped values, and the enclosing names rows carry.
use crate::tools::local_search::local_search_query;
use crate::tools::local_search::local_search_query::Multiline;
use crate::tools::local_search::local_search_query::View;

use super::super::enclosing::{
    ENCLOSING_FIELD_CHARS, ENCLOSING_RESERVE_CHARS, Outline, outlined_language,
};
use super::super::local_search_query::{effective_match_content_length, shows_span_columns};
use super::entries::{FILE_ENTRY_CHARS, digits, row_chars};
use super::expansion::expansion_read_chars;
use super::list_view;
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::types::SearchContext;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Serialized sizes a page is cut by. Rows render with workspace-relative
/// paths, so a file entry costs its root prefix too; a clipped row rides
/// its file's `next.expandValues*` read (see [`expand_values`]): the page
/// pays for that read, so it still fits.
pub(super) struct PageCosts {
    pub(super) prefix_chars: usize,
    /// matchOnly rows show their exact span clipped to the display bound.
    pub(super) match_only_limit: Option<usize>,
    /// Span rows carry `column` in the response (see [`shows_span_columns`]).
    pub(super) span_columns: bool,
    pub(super) expand_context: SearchContext,
    /// Multiline values are read once per file (see [`expansion_rows`]).
    pub(super) multiline: bool,
    pub(super) read_chars: usize,
    /// Rows are shown, so each hit row of an outlined language carries its
    /// `enclosing` name: the cut charges it (see [`PageCosts::name`]).
    pub(super) shows_names: bool,
    /// The names of each file the cut priced, by file index.
    pub(super) owners: HashMap<usize, FilePrice>,
}

/// The enclosing names one file's hit rows carry, as a page cut prices
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FilePrice {
    /// The owner of each hit line inside a declaration, by line (a line
    /// outside every declaration has none, as has every line of a file
    /// without an outline).
    Owners(Arc<HashMap<u32, Owner>>),
    /// The file could not be read for its outline: its rows reserve
    /// [`ENCLOSING_RESERVE_CHARS`] each.
    Unread,
}

impl FilePrice {
    /// The names of `file`'s hit rows by its `outline` (`None`: it has
    /// none). A row's owner is the one [`annotate_enclosing`] names it by,
    /// except that a declaration search names only its declaring rows (and
    /// a merged context block only its first line's owner): charging every
    /// hit row bounds those pages from above.
    pub(crate) fn of(
        file: &octocode_engine::types::TextSearchFile,
        outline: Option<&Outline>,
    ) -> Self {
        Self::Owners(Arc::new(
            outline
                .map(|outline| {
                    file.matches
                        .iter()
                        .filter_map(|matched| {
                            let owner = outline.label(outline.owner(matched.line)?)?;
                            let full = ENCLOSING_FIELD_CHARS
                                + octocode_engine::json_size::utf16_units(&owner).unwrap_or(0);
                            Some((matched.line, Owner { full }))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ))
    }

    /// Bytes this price keeps, as a stored scan weighs it.
    pub(crate) fn weight(&self) -> usize {
        match self {
            Self::Owners(owners) => owners.len() * std::mem::size_of::<(u32, Owner)>(),
            Self::Unread => 0,
        }
    }
}

/// Outlines `files` (by index) for a page cut and prices their rows'
/// names: one [`FilePrice`] per file.
pub(crate) type Outliner<'p> = dyn FnMut(
        &[(usize, &octocode_engine::types::TextSearchFile)],
    ) -> Result<Vec<(usize, FilePrice)>, ToolError>
    + 'p;

/// The enclosing names a page cut charges, priced on demand: a file is
/// outlined only once a cut must charge its rows exactly (see
/// [`Layout::cut`]); every other file of an outlined language reserves.
pub(crate) struct NamePricing<'p> {
    /// The prices known so far, by file index: a stored scan's, then the
    /// ones this cut outlined.
    pub(crate) priced: HashMap<usize, FilePrice>,
    outline: &'p mut Outliner<'p>,
}

impl<'p> NamePricing<'p> {
    pub(crate) fn new(priced: HashMap<usize, FilePrice>, outline: &'p mut Outliner<'p>) -> Self {
        Self { priced, outline }
    }

    /// Price the files of `wanted` that hold rows in an outlined language
    /// and have no price yet.
    pub(super) fn price(
        &mut self,
        files: &[octocode_engine::types::TextSearchFile],
        wanted: Vec<usize>,
    ) -> Result<(), ToolError> {
        let mut seen = HashSet::new();
        let unpriced = wanted
            .into_iter()
            .filter(|index| seen.insert(*index) && !self.priced.contains_key(index))
            .filter(|index| {
                let file = &files[*index];
                !file.matches.is_empty() && outlined_language(std::path::Path::new(&file.path))
            })
            .map(|index| (index, &files[index]))
            .collect::<Vec<_>>();
        if !unpriced.is_empty() {
            let mut outlined = (self.outline)(&unpriced)?
                .into_iter()
                .collect::<HashMap<_, _>>();
            for (index, _) in unpriced {
                // A file the outliner left out was not read.
                let price = outlined.remove(&index).unwrap_or(FilePrice::Unread);
                self.priced.insert(index, price);
            }
        }
        Ok(())
    }
}

/// A hit row's enclosing declaration as the page cut prices it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Owner {
    /// Serialized complete declaration, carried by every matching row.
    pub(super) full: usize,
}

/// What a hit row's enclosing name adds to a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowName {
    /// The row carries no name.
    None,
    /// A row of a file the cut has not outlined (or could not read):
    /// [`ENCLOSING_RESERVE_CHARS`].
    Reserved,
    /// The complete declaration.
    Owned(Owner),
}

impl RowName {
    pub(super) fn chars(self) -> usize {
        match self {
            Self::None => 0,
            Self::Reserved => ENCLOSING_RESERVE_CHARS,
            Self::Owned(owner) => owner.full,
        }
    }
}

impl PageCosts {
    pub(super) fn new(
        query: &LocalSearchQuery,
        paths: &PathPolicy,
        output_root: &std::path::Path,
        context: SearchContext,
    ) -> Self {
        let match_only = local_search_query::view(query) == View::MatchOnly;
        let prefix_chars = match paths.workspace_relative(output_root).as_deref() {
            Some(".") => 0,
            Some(relative) => octocode_engine::json_size::string_body_units(relative) + 1,
            None => {
                octocode_engine::json_size::string_body_units(&output_root.to_string_lossy()) + 1
            }
        };
        Self {
            prefix_chars,
            match_only_limit: match_only.then_some(effective_match_content_length(query) as usize),
            span_columns: shows_span_columns(query),
            expand_context: if match_only {
                SearchContext::default()
            } else {
                context
            },
            multiline: local_search_query::multiline(query) != Multiline::Off,
            shows_names: !list_view(local_search_query::view(query)),
            owners: HashMap::new(),
            read_chars: expansion_read_chars(
                query,
                local_search_query::multiline(query) != Multiline::Off,
            ),
        }
    }

    pub(super) fn clipped(&self, matched: &octocode_engine::types::TextSearchMatch) -> bool {
        match self.match_only_limit {
            Some(limit) => matched.value.chars().nth(limit).is_some(),
            None => matched.original_chars.is_some(),
        }
    }

    pub(super) fn file(&self, file: &octocode_engine::types::TextSearchFile) -> usize {
        let path = octocode_engine::json_size::string_body_units(&file.path) + self.prefix_chars;
        let entry = path + FILE_ENTRY_CHARS;
        if file.matches.iter().any(|m| self.clipped(m)) {
            entry + self.read_chars + path
        } else {
            entry
        }
    }

    /// The enclosing name a hit row of file `index` carries (priced independently): its owner in a file the cut outlined, a reserve in
    /// another outlined-language file (unread, or not priced yet), else
    /// none.
    pub(super) fn name(
        &self,
        index: usize,
        file: &octocode_engine::types::TextSearchFile,
        matched: &octocode_engine::types::TextSearchMatch,
    ) -> RowName {
        if !self.shows_names {
            return RowName::None;
        }
        match self.owners.get(&index) {
            Some(FilePrice::Owners(owners)) => owners
                .get(&matched.line)
                .map_or(RowName::None, |owner| RowName::Owned(*owner)),
            Some(FilePrice::Unread) => RowName::Reserved,
            None if outlined_language(std::path::Path::new(&file.path)) => RowName::Reserved,
            None => RowName::None,
        }
    }

    pub(super) fn row(&self, matched: &octocode_engine::types::TextSearchMatch) -> usize {
        let row = row_chars(matched, self.match_only_limit, self.span_columns);
        if self.clipped(matched) {
            // Its line range, and a share of the extra read every
            // `RANGES_MAX_ITEMS` ranges open.
            row + 2 * digits(matched.line.saturating_add(self.expand_context.after))
                + 4
                + self.read_chars.div_ceil(RANGES_MAX_ITEMS)
        } else {
            row
        }
    }
}
