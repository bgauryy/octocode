//! Importer recovery for TypeScript/JavaScript incoming queries.
//!
//! tsserver answers `references` and call hierarchy from the anchor file's
//! program. Two common layouts silently drop importers from that answer:
//! - an *inferred* project (no tsconfig/jsconfig includes the file, e.g. a
//!   `scripts/*.cjs` outside the package's `include`) holds only the opened
//!   files and what they import, never the files importing them;
//! - a CommonJS export (`module.exports = { f }`) is not followed back into a
//!   destructured `require` from the declaration side, even inside a config.
//!
//! This pass finds candidate files lexically, opens them, and keeps an
//! occurrence only when its definition chain resolves to the anchor's own
//! declaration: identity is proven by the server, never guessed from text.
//! Verified occurrences become extra anchors for the same request.
//!
//! Candidates are verified [`MAX_CANDIDATE_FILES`] per request. The sorted
//! candidate list is cut into windows of that size; `importerPage` picks the
//! window, and `next.nextImporterPage` (on each of the window's location
//! pages) carries the candidate digest as its `snapshot`, so every candidate is
//! verified in exactly one window and a changed list restarts the walk.
//! Before the list is cut, candidates whose imports name only other modules
//! are set aside as checked (`reach`): only files that may import the
//! anchor's module are opened.

use super::anchor::Position;
use super::failure::{LspFailure, flag_partial, ignored_text_lead};
use super::file_windows::FileWindows;
use super::import_stop::{proven_missing, stays_on_import};
use super::recovery::{resolve_definition_chain, snippet_identity};
use super::scope::{IgnoredMentions, Scope, canonical};
use super::source::SourceCache;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::lsp_search_query;
use futures_util::stream::{self, StreamExt, TryStreamExt};
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::client::{NativeLspClient, SnippetReadPolicy};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

mod binding;
mod coverage;
mod occurrences;
mod reach;
mod search;
pub(super) use binding::{Binding, binding, identities};
use occurrences::*;
pub(super) use occurrences::{CONCURRENT_FILE_VERIFICATIONS, ordered_concurrently};
use reach::{AnchorModule, read_checked, split_reachable};
#[cfg(test)]
use search::check_imports;
pub(super) use search::{
    ImporterSearch, SCAN_CAPPED, SCAN_COMPLETE, SCAN_FAILED, imports_name, verified_anchors,
};
/// Candidate files opened and verified per request: one importer window.
pub(super) const MAX_CANDIDATE_FILES: usize = 24;
/// Windows one request may verify. A window that holds no verified
/// importer is followed by the next in the same request (it adds no site),
/// up to this many windows or [`SKIP_BUDGET`] of wall time;
/// `next.nextImporterPage` resumes after them.
pub(super) const MAX_WINDOWS_PER_REQUEST: usize = 8;
/// Wall time after which a request that has skipped silent windows stops
/// skipping and returns its page of the continuing scan: one call stays
/// within the time of a few windows however slow the server is.
const SKIP_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);
/// Prefix of a candidate-list digest (the `snapshot` of
/// `next.nextImporterPage`).
const DIGEST_PREFIX: &str = "lsp-imp:";
/// Partial reason of a window that leaves later windows unverified.
pub(super) const CAPPED_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::ImporterScanCapped;
/// Partial reason of a scan whose ignore rules hid files that import the
/// name, or that spell it and could not be checked for an import.
pub(super) const IGNORED_REASON: crate::response::paging::PartialReason =
    crate::response::paging::PartialReason::ImporterScanIgnored;
/// The note of an ignored-file scan that did not finish before reaching any
/// evidence (its index not ready or incomplete): it reads the same on every
/// row of the scope, and says how to read those files.
const IGNORED_UNFINISHED_NOTICE: &str = "The scan for gitignored files that spell this name did not finish, so importers among them are unverified; a localSearch with includeIgnored reads them.";
/// Settle/ready bounds for the last candidate open of a window; the load it
/// triggers covers every candidate opened before it. Shorter than a
/// request's anchor open (the server already loaded the anchor's project),
/// so a request that verifies several windows stays bounded. They override
/// the engine's `OPEN_SETTLE_MS` and `OPEN_READY_TIMEOUT_MS` defaults.
const LAST_OPEN_SETTLE_MS: u32 = 200;
const LAST_OPEN_READY_TIMEOUT_MS: u32 = 10_000;

/// One verified occurrence (zero-based LSP position) in an importing file.
pub(super) struct Anchor {
    pub(super) path: String,
    pub(super) line: u32,
    pub(super) character: u32,
    /// The occurrence is a call site (TypeScript's call-hierarchy rule; see
    /// `classify_occurrences`), the anchor call hierarchy wants.
    pub(super) is_call: bool,
}

#[derive(Default)]
pub(super) struct Importers {
    pub(super) anchors: Vec<Anchor>,
    /// The importer window this request verified; `None` when no candidate
    /// list was computed.
    pub(super) window: Option<Window>,
    /// The request's `importerPage` snapshot no longer matches the
    /// candidate list: the caller restarts from the first window.
    pub(super) stale: bool,
    /// A candidate could not be read, opened, or resolved, or the scan failed.
    pub(super) failed: bool,
    /// Candidates whose every occurrence the server resolved to another
    /// declaration: checked, and not uses of this symbol.
    pub(super) rejected: Vec<String>,
    /// Candidates of this window the server could not resolve (a failed
    /// identity check, or a definition chain that never left the file while
    /// the declaration lives elsewhere): neither verified nor rejected, so
    /// they stay out of the answer and count as text-only files.
    pub(super) unverified: Vec<String>,
    /// Candidate sites whose name binds another declaration (a same-named
    /// binding of their own file, which shadows the anchor's): never a
    /// reference, whichever importer's answer lists them.
    pub(super) elsewhere: HashSet<Site>,
    /// Files that spell the name but that ignore rules kept out of the
    /// candidate scan: never verified, so the row names them.
    pub(super) ignored: IgnoredMentions,
}

/// Which importer window a request verifies (`importerPage`, one-based),
/// and the candidate digest a later window's first location page must
/// match (`snapshot`, copied from `next.nextImporterPage`).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WindowRequest<'a> {
    pub(super) page: u32,
    pub(super) expected: Option<&'a str>,
}

impl<'a> WindowRequest<'a> {
    pub(super) fn of(query: &'a LspSearchQuery) -> Self {
        let page = lsp_search_query::importer_page(query);
        // A later location page carries its location snapshot instead; that
        // snapshot is salted with the candidate digest (see [`Window::digest`]).
        let entering = page > 1 && lsp_search_query::page(query).unwrap_or(1) == 1;
        Self {
            page,
            expected: entering.then(|| lsp_search_query::snapshot(query).unwrap_or_default()),
        }
    }
}

/// The importer windows one request verified, of the sorted candidate list.
#[derive(Debug)]
pub(super) struct Window {
    /// One-based index of the first window the request verified (its
    /// `importerPage`) and of the last one it verified. The two differ when
    /// windows without a verified importer were skipped.
    pub(super) first: u32,
    pub(super) page: u32,
    /// Every candidate, sorted, in windows of [`MAX_CANDIDATE_FILES`],
    /// pinned to the list's digest.
    list: FileWindows<String>,
    /// Candidates no window verifies: files the server's own answer already
    /// covers, and files whose imports cannot reach the anchor's module.
    /// They keep their place in the list, so the windows and the digest
    /// depend on the text scan alone, never on what the server answered.
    excluded: HashSet<String>,
}

/// A window cut from the candidate list, or a stale request.
#[derive(Debug)]
pub(super) enum Cut {
    Window(Window),
    Stale,
}

impl Window {
    /// Cut window `request.page` from the sorted `candidates`. A request for
    /// a window past the last, or whose expected digest differs, is stale.
    pub(super) fn cut(candidates: Vec<String>, request: WindowRequest<'_>) -> Cut {
        let digest = format!(
            "{DIGEST_PREFIX}{}",
            octocode_engine::digest::sha256(candidates.join("\u{0}").as_bytes())
        );
        let list = FileWindows::new(candidates, MAX_CANDIDATE_FILES, digest);
        let page = request.page.max(1);
        if !list.admits(page, request.expected) {
            return Cut::Stale;
        }
        Cut::Window(Self {
            first: page,
            page,
            list,
            excluded: HashSet::new(),
        })
    }

    /// Digest of the whole candidate list.
    pub(super) fn digest(&self) -> &str {
        self.list.digest()
    }

    /// Candidates in the whole list.
    pub(super) fn total(&self) -> usize {
        self.list.len()
    }

    /// The window count.
    #[cfg(test)]
    pub(super) fn pages(&self) -> u32 {
        self.list.pages()
    }

    /// Leave `excluded` out of every window's verification (see the field).
    pub(super) fn exclude(&mut self, excluded: HashSet<String>) {
        self.excluded = excluded;
    }

    pub(super) fn range(&self, page: u32) -> std::ops::Range<usize> {
        self.list.range(page)
    }

    /// The candidates of the window verified last, less the excluded ones.
    pub(super) fn files(&self) -> Vec<String> {
        self.list.items()[self.range(self.page)]
            .iter()
            .filter(|file| !self.excluded.contains(*file))
            .cloned()
            .collect()
    }

    /// Verify the next window as well: the last one held no importer.
    fn advance(&mut self) {
        self.page += 1;
    }

    /// Windows this request has verified so far.
    fn verified_windows(&self) -> usize {
        (self.page - self.first) as usize + 1
    }

    /// One-based candidate numbers `(first, last)` this request verified.
    pub(super) fn verified_candidates(&self) -> (usize, usize) {
        (self.range(self.first).start + 1, self.range(self.page).end)
    }

    /// Candidates of this window and every earlier one: the files the walk
    /// has verified once this page is read.
    pub(super) fn covered(&self) -> &[String] {
        &self.list.items()[..self.range(self.page).end]
    }

    /// Candidates of every window this request did not verify.
    pub(super) fn foreign(&self) -> impl Iterator<Item = &String> {
        let (start, end) = (self.range(self.first).start, self.range(self.page).end);
        let items = self.list.items();
        items[..start].iter().chain(&items[end..])
    }

    /// Whether a file (canonical path) belongs to a window this request
    /// verified: a candidate's locations belong to its own window, and a file
    /// outside the list to the last one.
    fn verified_owner(&self, file: &str) -> bool {
        (self.first..=self.page).contains(&self.owner(file))
    }

    /// The window that owns a file (canonical path): its candidate window,
    /// or this one for a file outside the list.
    pub(super) fn owner(&self, file: &str) -> u32 {
        self.list
            .items()
            .binary_search_by(|candidate| candidate.as_str().cmp(file))
            .map_or(self.page, |index| self.list.page_of(index))
    }

    /// Whether a later window remains.
    pub(super) fn more(&self) -> bool {
        self.list.more_after(self.page)
    }
}

/// True for operations whose answer lists places that point *at* the anchor.
pub(super) fn applies(language_id: Option<&str>, operation: &str) -> bool {
    octocode_engine::lsp::family::is_typescript_family(language_id)
        && matches!(operation, "references" | "callers")
}

/// Zero-based (line, UTF-16 character, is-call) of word-bounded `symbol`
/// occurrences; is-call is the text rule (`(` follows the name), which
/// `classify_occurrences` replaces wherever oxc parses the file.
pub(super) fn occurrences(content: &str, symbol: &str) -> Vec<(u32, u32, bool)> {
    let mut found = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        let mut from = 0;
        while let Some(offset) = line[from..].find(symbol) {
            let start = from + offset;
            let end = start + symbol.len();
            from = end;
            if !octocode_engine::text::is_whole_word(line.as_bytes(), start, end) {
                continue;
            }
            let (Ok(line_number), Ok(character)) = (
                u32::try_from(line_index),
                u32::try_from(line[..start].encode_utf16().count()),
            ) else {
                continue;
            };
            let is_call = line[end..].trim_start().starts_with('(');
            found.push((line_number, character, is_call));
        }
    }
    found
}

/// TS/JS files under the request's search scope that mention `symbol` as
/// a word, excluding `skip`, sorted. The scan is the scope's text scan
/// (shared with `textOnlyFiles`) and observes `cancel`; `None` means the
/// scan itself failed, which is neither cancellation nor an empty
/// candidate set.
async fn candidate_files(
    scope: &Scope,
    symbol: &str,
    skip: &HashSet<String>,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<Vec<String>>, LspFailure> {
    let Some(files) = scope.text_files(symbol, policy, cancel).await? else {
        return Ok(None);
    };
    Ok(Some(
        files
            .iter()
            .filter(|path| !skip.contains(*path))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    ))
}

#[cfg(test)]
mod tests;
