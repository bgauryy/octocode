//! What a page cut charges: file entries, complete rows and enclosing names.
use crate::local_search::local_search_query;

use super::super::enclosing::{
    ENCLOSING_FIELD_CHARS, ENCLOSING_RESERVE_CHARS, Outline, outlined_language,
};
use super::super::local_search_query::shows_span_columns;
use super::entries::{FILE_ENTRY_CHARS, row_chars};
use super::list_view;
use octocode_contract_types::LocalSearchQuery;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::result::ToolError;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Serialized sizes a page is cut by. File entries include the workspace-relative prefix.
pub(super) struct PageCosts {
    pub(super) prefix_chars: usize,
    /// A file entry's time under a time order (`,"modifiedAt":"…Z"`,
    /// [`local_search_query::sort_time_key`]); 0 under any other.
    sorted_at_chars: usize,
    /// The root a shown file's path is named from: a scanned file's path
    /// still spells it until the page names the file from it.
    root: std::path::PathBuf,
    /// Span rows carry `column` in the response (see [`shows_span_columns`]).
    pub(super) span_columns: bool,
    /// Rows are shown, so each hit row of an outlined language carries its
    /// `enclosing` name: the cut charges it (see [`PageCosts::name`]).
    pub(super) shows_names: bool,
    /// The names of each file the cut priced, by file index.
    pub(super) owners: HashMap<usize, FilePrice>,
    /// The term a span equal to it leaves out of its row
    /// ([`local_search_query::derived_value`]): such a row costs no `value`.
    derived: Option<String>,
}

/// The enclosing names one file's hit rows carry, as a page cut prices
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FilePrice {
    /// The owner of each hit line inside a declaration, by line (a line
    /// outside every declaration has none, as has every line of a file
    /// without an outline).
    Owners(Arc<HashMap<(u32, u32), Owner>>),
    /// The file could not be read for its outline: its rows reserve
    /// [`ENCLOSING_RESERVE_CHARS`] each.
    Unread,
}

impl FilePrice {
    /// The names of `file`'s hit rows by its `outline` (`None`: it has
    /// none). A row's owner is the one [`annotate_enclosing`](crate::local_search::enclosing::annotate_enclosing) names it by,
    /// except that a declaration search names only its declaring rows (and
    /// a merged context block lists each owner once, with one index per
    /// hit): charging every hit row its owner whole bounds those pages from
    /// above.
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
                            let at = (matched.line, matched.byte_offset);
                            let owner = outline.label(outline.owner(at.0, at.1)?)?;
                            let full = ENCLOSING_FIELD_CHARS
                                + octocode_engine::json_size::utf16_units(&owner).unwrap_or(0);
                            Some((at, Owner { full }))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ))
    }

    /// Bytes this price keeps, as a stored scan weighs it.
    pub(crate) fn weight(&self) -> usize {
        match self {
            Self::Owners(owners) => owners.len() * std::mem::size_of::<((u32, u32), Owner)>(),
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
/// [`Layout::cut`](super::Layout::cut)); every other file of an outlined language reserves.
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
    ) -> Self {
        let prefix_chars = match paths.workspace_relative(output_root).as_deref() {
            Some(".") => 0,
            Some(relative) => octocode_engine::json_size::string_body_units(relative) + 1,
            None => {
                octocode_engine::json_size::string_body_units(&output_root.to_string_lossy()) + 1
            }
        };
        Self {
            prefix_chars,
            // `,"<key>":"<time>"`, the time to the second.
            sorted_at_chars: local_search_query::sort_time_key(query)
                .map_or(0, |key| format!(r#","{key}":"YYYY-MM-DDTHH:MM:SSZ""#).len()),
            root: output_root.to_path_buf(),
            span_columns: shows_span_columns(query),
            shows_names: {
                let view = local_search_query::view(query);
                !list_view(view) && view != local_search_query::View::MatchOnly
            },
            owners: HashMap::new(),
            derived: local_search_query::derived_value(query).map(str::to_owned),
        }
    }

    /// A file entry's chars, its path named from the root as the page shows
    /// it: a file as the scan found it (under the root) and as the page
    /// shows it (from the root) are priced alike, so the stream's test and
    /// the cut agree ([`super::page_fill`]).
    pub(super) fn file(&self, file: &octocode_engine::types::TextSearchFile) -> usize {
        let shown = std::path::Path::new(&file.path)
            .strip_prefix(&self.root)
            .map_or(std::borrow::Cow::Borrowed(file.path.as_str()), |relative| {
                relative.to_string_lossy()
            });
        octocode_engine::json_size::string_body_units(&shown)
            + self.prefix_chars
            + FILE_ENTRY_CHARS
            + self.sorted_at_chars
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
                .get(&(matched.line, matched.byte_offset))
                .map_or(RowName::None, |owner| RowName::Owned(*owner)),
            Some(FilePrice::Unread) => RowName::Reserved,
            None if outlined_language(std::path::Path::new(&file.path)) => RowName::Reserved,
            None => RowName::None,
        }
    }

    pub(super) fn row(&self, matched: &octocode_engine::types::TextSearchMatch) -> usize {
        #[cfg(test)]
        PRICED.with(|priced| priced.set(priced.get() + 1));
        let chars = row_chars(matched, self.span_columns);
        match self.derived.as_deref() {
            // The row as published: `,"value":"<term>"` left out.
            Some(term) if matched.deferred.is_none() && matched.value == term => chars
                .saturating_sub(
                    VALUE_FIELD_CHARS + octocode_engine::json_size::string_body_units(term),
                ),
            _ => chars,
        }
    }
}

/// Serialized chars of a row's empty `value` field with its separator.
const VALUE_FIELD_CHARS: usize = ",\"value\":\"\"".len();

#[cfg(test)]
thread_local! {
    /// Rows this thread's page cuts priced, for tests that pin how much of a
    /// scan a page prices.
    static PRICED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Rows priced by page cuts on this thread so far.
#[cfg(test)]
pub(crate) fn priced_on_this_thread() -> usize {
    PRICED.with(std::cell::Cell::get)
}
