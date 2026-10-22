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
//! window, and `next.nextImporterPage` (on the window's last location page)
//! carries the candidate digest as its `snapshot`, so every candidate is
//! verified in exactly one window and a changed list restarts the walk.

use super::LspSearchQuery;
use super::anchor::Position;
use super::failure::{LspFailure, flag_partial, ignored_text_lead};
use super::import_stop::{proven_missing, stays_on_import};
use super::recovery::{resolve_definition_chain, snippet_identity};
use super::scope::{CODE_CHECK_MAX_BYTES, CODE_CHECK_MAX_FILES, IgnoredMentions, Scope, canonical};
use super::source::SourceCache;
use crate::execution::cancel::CancellationCheck;
use crate::num::u32_of_usize;
use crate::policy::path::PathPolicy;
use crate::response::result::Confidence;
use crate::tools::lsp_search::lsp_search_query;
use futures_util::stream::{self, StreamExt, TryStreamExt};
use octocode_engine::lsp::client::{NativeLspClient, SnippetReadPolicy};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;
/// Candidate files opened and verified per request: one importer window.
pub(super) const MAX_CANDIDATE_FILES: usize = 24;
/// Windows one request may verify. A later window that holds no verified
/// importer is skipped in the same request (its page would list no site),
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
pub(super) const CAPPED_REASON: &str = "importerScanCapped";
/// Partial reason of a scan whose ignore rules hid files that import the
/// name, or that spell it and could not be checked for an import.
pub(super) const IGNORED_REASON: &str = "importerScanIgnored";
/// The note of a lifted scan that stopped at its bound before reaching any
/// evidence: it reads the same on every row of the scope.
const IGNORED_UNFINISHED_NOTICE: &str = "The scan for gitignored files that spell this name stopped at its time bound, so importers among them are unverified.";
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
    /// The occurrence is a call (`name(`), the anchor call hierarchy wants.
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
    /// `importerPage`), of the last one it verified, and the window count.
    /// The two differ when windows without a verified importer were skipped.
    pub(super) first: u32,
    pub(super) page: u32,
    pub(super) pages: u32,
    /// Digest of the whole candidate list.
    pub(super) digest: String,
    /// Candidates in the whole list.
    pub(super) total: usize,
    /// Every candidate, sorted; window `n` is the `n`th run of
    /// [`MAX_CANDIDATE_FILES`].
    candidates: Vec<String>,
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
        let total = candidates.len();
        let pages = u32_of_usize(total.div_ceil(MAX_CANDIDATE_FILES).max(1));
        let page = request.page.max(1);
        if page > pages || request.expected.is_some_and(|expected| expected != digest) {
            return Cut::Stale;
        }
        Cut::Window(Self {
            first: page,
            page,
            pages,
            digest,
            total,
            candidates,
        })
    }

    fn range(&self, page: u32) -> std::ops::Range<usize> {
        let start = (page as usize - 1) * MAX_CANDIDATE_FILES;
        start.min(self.total)..(start + MAX_CANDIDATE_FILES).min(self.total)
    }

    /// The candidates of the window verified last.
    pub(super) fn files(&self) -> &[String] {
        &self.candidates[self.range(self.page)]
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
        &self.candidates[..self.range(self.page).end]
    }

    /// Candidates of every window this request did not verify.
    pub(super) fn foreign(&self) -> impl Iterator<Item = &String> {
        let (start, end) = (self.range(self.first).start, self.range(self.page).end);
        self.candidates[..start]
            .iter()
            .chain(&self.candidates[end..])
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
        self.candidates
            .binary_search_by(|candidate| candidate.as_str().cmp(file))
            .map_or(self.page, |index| {
                u32_of_usize(index / MAX_CANDIDATE_FILES + 1)
            })
    }

    /// Whether a later window remains.
    pub(super) fn more(&self) -> bool {
        self.page < self.pages
    }
}

/// True for operations whose answer lists places that point *at* the anchor.
pub(super) fn applies(language_id: Option<&str>, operation: &str) -> bool {
    octocode_engine::lsp::family::is_typescript_family(language_id)
        && matches!(operation, "references" | "callers")
}

/// Zero-based (line, UTF-16 character, is-call) of word-bounded `symbol`
/// occurrences; is-call means the name is followed by `(`.
fn occurrences(content: &str, symbol: &str) -> Vec<(u32, u32, bool)> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut found = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        let mut from = 0;
        while let Some(offset) = line[from..].find(symbol) {
            let start = from + offset;
            let end = start + symbol.len();
            from = end;
            let before = line[..start].chars().next_back();
            let after = line[end..].chars().next();
            if before.is_some_and(is_word) || after.is_some_and(is_word) {
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

impl Importers {
    /// One anchor per file for location requests (`references`).
    pub(super) fn per_file(&self) -> impl Iterator<Item = &Anchor> {
        let mut seen = HashSet::new();
        self.anchors
            .iter()
            .filter(move |anchor| seen.insert(anchor.path.clone()))
    }

    /// One anchor per file for call hierarchy, preferring a call site.
    pub(super) fn call_sites(&self) -> Vec<&Anchor> {
        let mut chosen: Vec<&Anchor> = Vec::new();
        for anchor in &self.anchors {
            match chosen.iter_mut().find(|kept| kept.path == anchor.path) {
                Some(kept) if anchor.is_call && !kept.is_call => *kept = anchor,
                Some(_) => {}
                None => chosen.push(anchor),
            }
        }
        chosen
    }

    /// Whether a recovered location in `file` (canonical) belongs to the
    /// windows this request verified: a candidate's locations belong to its
    /// own window.
    pub(super) fn owns(&self, file: &str) -> bool {
        self.window
            .as_ref()
            .is_none_or(|window| window.verified_owner(file))
    }

    /// The candidate digest, salted into this request's location snapshot so
    /// a changed candidate list stales the window's later location pages.
    pub(super) fn digest(&self) -> Option<&str> {
        self.window.as_ref().map(|window| window.digest.as_str())
    }

    /// Candidates checked by this window and every earlier one, less this
    /// window's unverified candidates: those stay text-only so the row's
    /// coverage and lexical lead disclose them.
    pub(super) fn covered(&self) -> Vec<String> {
        let unverified = self.unverified.iter().collect::<HashSet<_>>();
        self.window
            .as_ref()
            .map_or(&[][..], Window::covered)
            .iter()
            .filter(|file| !unverified.contains(file))
            .cloned()
            .collect()
    }

    /// A page whose windows held no verified importer lists no site. While
    /// the scan goes on (or after earlier windows), that is one step of the
    /// scan, not an empty answer: the row stops being `empty` (no exit-1
    /// outcome, no re-anchor hint) and says which candidates were checked
    /// and whether the scan goes on. Only a first page that ends the scan
    /// with no site is a true empty answer.
    fn settle_silent_page(&self, row: &mut Value, query: &LspSearchQuery, window: &Window) {
        if (window.first <= 1 && !window.more())
            || row.get("status").and_then(Value::as_str) != Some("empty")
        {
            return;
        }
        let (from, to) = window.verified_candidates();
        let held = if self.anchors.is_empty() {
            "held no reference to this symbol"
        } else {
            "held no reference to this symbol that an earlier page does not list"
        };
        let tail = match (window.more(), self.failed) {
            (true, _) => "the scan continues: the importer lead verifies the following candidates",
            // Candidates that could not be read are not verified: the scan
            // ended, but it is not complete (the warning names the failure).
            (false, true) => {
                "that was the last window; the candidates the scan could not read stay unchecked"
            }
            (false, false) => "that was the last window, so the importer scan is complete",
        };
        let Some(object) = row.as_object_mut() else {
            return;
        };
        for field in ["status", "category", "hints"] {
            object.remove(field);
        }
        object.insert("kind".into(), json!(lsp_search_query::operation(query)));
        object.insert("matches".into(), json!([]));
        object.insert(
            "reason".into(),
            json!(format!(
                "Candidates {from}-{to} of {} {held}; {tail}.",
                window.total
            )),
        );
    }

    /// Gitignored files are outside the candidate scan. One that imports the
    /// name may be a hidden importer, and one the import check could not
    /// decide is not ruled out: the row is partial, counts each kind, names
    /// the deciding rules, and leads to exactly those files. Files that only
    /// spell the name are no evidence of a hidden importer: the row keeps
    /// only the lead over them. A scan that stopped at its bound left the
    /// rest unverified: its lead is over the scope, which holds the files it
    /// found and those it did not reach, and without evidence the row is
    /// partial and says the scan did not finish. Nested repositories the scan
    /// did not enter are counted, and the lead reaches into each of them (a
    /// row with no name to search names them instead); they are no evidence
    /// on their own, so they never make the row partial.
    fn disclose_ignored(&self, row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
        let ignored = &self.ignored;
        if !ignored.nested_repositories.is_empty() {
            // The `includeSkipped` lead lists each one; without a name to
            // search there is no lead, so the notice names them.
            let listed = if lsp_search_query::symbol_name(query).is_some() {
                String::new()
            } else {
                format!(": {}", ignored.nested_repositories.join(", "))
            };
            crate::response::paging::push_notice(
                row,
                format!(
                    "The scan for gitignored files did not enter {} (directories with their own .git) under the scope root{listed}.",
                    match ignored.nested_repositories.len() {
                        1 => "1 nested repository".to_owned(),
                        count => format!("{count} nested repositories"),
                    },
                ),
            );
        }
        let Some(name) = lsp_search_query::symbol_name(query).filter(|_| {
            !ignored.files.is_empty()
                || ignored.unfinished
                || !ignored.nested_repositories.is_empty()
        }) else {
            return;
        };
        let evidence = ignored
            .importing
            .iter()
            .chain(&ignored.unchecked)
            .cloned()
            .collect::<Vec<_>>();
        let spelled = if evidence.is_empty() {
            &ignored.files
        } else {
            &evidence
        };
        let nested = ignored.nested_repositories.iter().map(|repo| {
            Path::new(&scope.root)
                .join(repo)
                .to_string_lossy()
                .into_owned()
        });
        let listed = spelled.iter().cloned().chain(nested).collect::<Vec<_>>();
        row["next"]["includeSkipped"] = ignored_text_lead(name, scope, &listed, ignored.unfinished);
        if evidence.is_empty() {
            if ignored.unfinished {
                crate::response::paging::mark_partial(row, IGNORED_REASON, &[]);
                crate::response::paging::push_notice(row, IGNORED_UNFINISHED_NOTICE);
            }
            return;
        }
        let rules = match ignored.rules.as_slice() {
            [] => "ignore rules".to_owned(),
            rules => rules.join(", "),
        };
        let mut counts = Vec::new();
        match ignored.importing.len() {
            0 => {}
            1 => counts.push("1 imports it".to_owned()),
            count => counts.push(format!("{count} import it")),
        }
        if !ignored.unchecked.is_empty() {
            counts.push(format!(
                "{} could not be checked for an import (unreadable, larger than {} MiB, unparsable, or past the first {CODE_CHECK_MAX_FILES} checked)",
                ignored.unchecked.len(),
                CODE_CHECK_MAX_BYTES / (1024 * 1024),
            ));
        }
        let more = if ignored.unfinished {
            " The scan for gitignored files stopped at its time bound, so more may exist."
        } else {
            ""
        };
        let warning = format!(
            "Of the gitignored files that spell this name ({rules}), {}; importer recovery did not verify them, and a localSearch with includeIgnored reads their text.{more}",
            counts.join(" and ")
        );
        crate::response::paging::mark_partial(row, IGNORED_REASON, &[warning]);
    }

    /// Record the scan on the row's coverage so readers and
    /// `inferred_project::annotate` know importers were checked. A window
    /// with later windows left is partial (not a terminal limit) and, on
    /// its last location page, carries `next.nextImporterPage`.
    pub(super) fn annotate(&self, row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
        if row.get("status").and_then(Value::as_str) == Some("error") {
            return;
        }
        let files = self.per_file().count();
        let more = self.window.as_ref().is_some_and(Window::more);
        if let Some(payload) = row.as_object_mut() {
            let coverage = payload.entry("coverage").or_insert_with(|| json!({}));
            coverage["importerScan"] = json!(if self.failed {
                SCAN_FAILED
            } else if more {
                SCAN_CAPPED
            } else {
                SCAN_COMPLETE
            });
            coverage["verifiedImporterFiles"] = json!(files);
        }
        if let Some(window) = &self.window {
            self.settle_silent_page(row, query, window);
        }
        self.disclose_ignored(row, query, scope);
        let Some(window) = self.window.as_ref().filter(|window| window.more()) else {
            return;
        };
        let Some(importer_page) = crate::response::paging::next_cursor(
            row,
            crate::id::ToolId::LspSearch,
            None,
            "importerPage",
            usize::try_from(window.page).map_or(usize::MAX, |page| page.saturating_add(1)),
        ) else {
            return;
        };
        let range = window.range(window.page + 1);
        let last_location_page =
            row.pointer("/pagination/hasMore").and_then(Value::as_bool) != Some(true);
        if !self.failed {
            let (from, to) = window.verified_candidates();
            let warning = format!(
                "{total} candidate files mention this name outside the server's answer; importer recovery verifies {MAX_CANDIDATE_FILES} at a time. Verified candidates {from}-{to} of {total}; the importer lead (offered on the last page of sites) verifies candidates {}-{}.",
                range.start + 1,
                range.end,
                total = window.total,
            );
            flag_partial(row, query, CAPPED_REASON, Some(&warning), scope);
        }
        if !last_location_page {
            return;
        }
        let mut next = lsp_search_query::to_row(query);
        if let Some(fields) = next.as_object_mut() {
            fields.remove("page");
            fields.insert("importerPage".into(), json!(importer_page));
            fields.insert("snapshot".into(), json!(window.digest));
        }
        row["next"]["nextImporterPage"] =
            crate::response::result::Continuation::new(crate::id::ToolId::LspSearch, next)
                .confidence(Confidence::Exact)
                .build();
    }
}

pub(super) const SCAN_COMPLETE: &str = "complete";
pub(super) const SCAN_CAPPED: &str = "capped";
pub(super) const SCAN_FAILED: &str = "failed";

/// What [`verified_anchors`] verifies: importers of `symbol`, declared at
/// `declaration`, among `scope`'s candidates in the importer `window` the
/// request names. `known_files` are files the server already reported; they
/// are not candidates.
pub(super) struct ImporterSearch<'a> {
    pub(super) symbol: &'a str,
    pub(super) scope: &'a Scope,
    pub(super) declaration: Position<'a>,
    pub(super) known_files: &'a HashSet<String>,
    pub(super) window: WindowRequest<'a>,
}

/// Verified importer anchors of one [`ImporterSearch`].
pub(super) async fn verified_anchors(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    search: ImporterSearch<'_>,
) -> Result<Importers, LspFailure> {
    let ImporterSearch {
        symbol,
        scope,
        declaration:
            Position {
                path: anchor_path,
                line,
                character,
            },
        known_files,
        window: request,
    } = search;
    if symbol.trim().is_empty() || scope.root.is_empty() {
        // No candidate list: a later window cannot be placed.
        return Ok(Importers {
            stale: request.page > 1,
            ..Importers::default()
        });
    }
    let mut skip = known_files.clone();
    skip.insert(canonical(anchor_path));
    let Some(candidates) = candidate_files(scope, symbol, &skip, sources.policy(), cancel).await?
    else {
        return Ok(Importers {
            failed: true,
            stale: request.page > 1,
            ..Importers::default()
        });
    };
    let window = match Window::cut(candidates, request) {
        Cut::Window(window) => window,
        Cut::Stale => {
            return Ok(Importers {
                stale: true,
                ..Importers::default()
            });
        }
    };
    let mut ignored = scope
        .ignored_text_files(symbol, &skip, sources.policy(), cancel)
        .await?;
    check_imports(&mut ignored, sources.policy(), cancel, symbol).await?;
    let declaration = identities(
        client,
        sources,
        snippet_policy,
        cancel,
        anchor_path,
        line,
        character,
    )
    .await?;
    let Some(declaration) = declaration.filter(|declaration| !declaration.is_empty()) else {
        return Ok(Importers {
            window: Some(window),
            failed: true,
            ignored,
            ..Importers::default()
        });
    };
    let mut window = window;
    let started = std::time::Instant::now();
    let mut anchors = Vec::new();
    let mut rejected = Vec::new();
    let mut unverified = Vec::new();
    loop {
        let (opened, open_failed) =
            open_candidates(client, sources, cancel, anchor_path, window.files(), symbol).await?;
        let (found, other, unresolved) = verify_occurrences(
            client,
            sources,
            snippet_policy,
            cancel,
            opened,
            &declaration,
        )
        .await?;
        anchors.extend(found);
        rejected.extend(other);
        unverified.extend(unresolved.into_iter().chain(open_failed));
        // The first window lists the anchor's own answer; a later one with
        // no verified importer would list nothing, so the request goes on.
        let silent = window.first > 1 && anchors.is_empty();
        if !silent
            || !window.more()
            || window.verified_windows() >= MAX_WINDOWS_PER_REQUEST
            || started.elapsed() >= SKIP_BUDGET
        {
            break;
        }
        window.advance();
    }
    Ok(Importers {
        anchors,
        window: Some(window),
        stale: false,
        failed: !unverified.is_empty(),
        rejected,
        unverified,
        ignored,
    })
}

/// Sort `ignored.files` into those that import `symbol` (by name:
/// `import { symbol }`, `const { symbol } = require(…)`, renamed or not; or
/// through a namespace or default binding whose member `.symbol` the file
/// spells) and those the check cannot decide: refused by the policy,
/// unreadable, larger than [`CODE_CHECK_MAX_BYTES`], unparsable, or past
/// [`CODE_CHECK_MAX_FILES`]. One blocking worker reads the files in turn and
/// keeps none: the server never opens them, so no request cache holds them.
async fn check_imports(
    ignored: &mut IgnoredMentions,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
    symbol: &str,
) -> Result<(), LspFailure> {
    let files = ignored.files.clone();
    let (policy, symbol) = (policy.clone(), symbol.to_owned());
    let checked = super::blocking_cancellable(cancel, move |stopped| {
        files
            .into_iter()
            .enumerate()
            .map(|(index, file)| {
                let imports = (index < CODE_CHECK_MAX_FILES && !stopped())
                    .then(|| import_of(&policy, &file, &symbol))
                    .flatten();
                (file, imports)
            })
            .collect::<Vec<_>>()
    })
    .await?;
    let Some(checked) = checked else {
        // The worker failed: nothing was decided.
        ignored.unchecked = ignored.files.clone();
        return Ok(());
    };
    for (file, imports) in checked {
        match imports {
            Some(true) => ignored.importing.push(file),
            Some(false) => {}
            None => ignored.unchecked.push(file),
        }
    }
    Ok(())
}

/// Whether the policy-readable, bounded `file` imports `symbol`; `None`
/// when it cannot be read or parsed.
fn import_of(policy: &PathPolicy, file: &str, symbol: &str) -> Option<bool> {
    let valid = policy.validate_read(file).ok()?;
    let bytes = crate::tools::source::read_bounded(&valid.canonical, CODE_CHECK_MAX_BYTES).ok()?;
    imports_name(&String::from_utf8(bytes).ok()?, file, symbol)
}

/// Whether `content` imports `symbol` (see [`check_imports`]); `None` when
/// the file cannot be parsed.
fn imports_name(content: &str, file: &str, symbol: &str) -> Option<bool> {
    let parsed = octocode_engine::signatures::extract_graph_facts(content, file)
        .and_then(|facts| serde_json::from_str::<Value>(&facts).ok())?;
    let member = format!(".{symbol}");
    let spells_member = || {
        content.match_indices(&member).any(|(at, _)| {
            !content[at + member.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        })
    };
    Some(
        parsed
            .get("imports")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(
                |import| match import.get("importedName").and_then(Value::as_str) {
                    Some(name) if name == symbol => true,
                    Some("*" | "default") => spells_member(),
                    _ => false,
                },
            ),
    )
}

/// A synchronized candidate and its `(line, character, is-call)` occurrences.
type OpenedFile = (String, Vec<(u32, u32, bool)>);

/// Open every candidate that mentions `symbol`; wait once, on the last one
/// opened, for the project load the opens trigger, so identity checks do
/// not race it. The anchor document is synchronized first: the client
/// keeps a bounded set of open documents, and a request that verifies
/// several windows must not evict the anchor whose program the checks use.
/// Returns each opened file's occurrences and the candidates that failed.
async fn open_candidates(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    anchor: &str,
    files: &[String],
    symbol: &str,
) -> Result<(Vec<OpenedFile>, Vec<String>), LspFailure> {
    if let Some(source) = sources.get(anchor).await {
        client
            .open_document(anchor.to_owned(), &source.content)
            .await
            .map_err(|error| LspFailure::from_engine(&error))?;
    }
    let mut eligible = Vec::new();
    let mut failed = Vec::new();
    for file in files {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(file).await else {
            failed.push(file.clone());
            continue;
        };
        let spots = occurrences(&source.content, symbol);
        if !spots.is_empty() {
            eligible.push((file.clone(), spots));
        }
    }
    let last = eligible.len().saturating_sub(1);
    let mut opened = Vec::new();
    for (index, (file, spots)) in eligible.into_iter().enumerate() {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(&file).await else {
            failed.push(file);
            continue;
        };
        let synced = if index == last {
            client
                .open_document_and_wait(
                    file.clone(),
                    &source.content,
                    Some(LAST_OPEN_SETTLE_MS),
                    Some(LAST_OPEN_READY_TIMEOUT_MS),
                )
                .await
                .map(|_| ())
        } else {
            client.open_document(file.clone(), &source.content).await
        };
        if synced.is_ok() {
            opened.push((file, spots));
        } else {
            failed.push(file);
        }
    }
    Ok((opened, failed))
}

/// Keep each occurrence whose definition chain reaches `declaration`.
/// Returns the verified anchors, the files whose every occurrence resolved
/// to another declaration (rejected), and the files left unverified: an
/// identity check failed, or every occurrence's chain stopped inside the
/// file itself (an import binding the server did not follow) while the
/// declaration lives elsewhere. Unresolved is not proof of another
/// declaration, so such a file is never rejected.
async fn verify_occurrences(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    opened: Vec<OpenedFile>,
    declaration: &HashSet<String>,
) -> Result<(Vec<Anchor>, Vec<String>, Vec<String>), LspFailure> {
    let policy = sources.policy();
    let missing = proven_missing(policy);
    // A server that pipelines requests (tsgo) gets several files at once,
    // each with its own source cache (a cache needs `&mut`); the answers
    // keep candidate order. tsserver works through requests one at a time,
    // so there each file waits its turn on the shared cache instead of
    // queueing behind the others against its request timeout.
    let verdicts = if client.serves_requests_concurrently() {
        ordered_concurrently(opened, CONCURRENT_FILE_VERIFICATIONS, |file| async {
            let mut own = SourceCache::new(policy);
            verify_file(
                client,
                &mut own,
                snippet_policy,
                cancel,
                file,
                declaration,
                &missing,
            )
            .await
        })
        .await?
    } else {
        let mut verdicts = Vec::with_capacity(opened.len());
        for file in opened {
            verdicts.push(
                verify_file(
                    client,
                    sources,
                    snippet_policy,
                    cancel,
                    file,
                    declaration,
                    &missing,
                )
                .await?,
            );
        }
        verdicts
    };
    let mut anchors = Vec::new();
    let mut rejected = Vec::new();
    let mut unverified = Vec::new();
    for verdict in verdicts {
        if !verdict.anchors.is_empty() {
            anchors.extend(verdict.anchors);
        } else if verdict.unresolved {
            unverified.push(verdict.file);
        } else {
            rejected.push(verdict.file);
        }
    }
    Ok((anchors, rejected, unverified))
}

/// Run `task` over `items` with at most `limit` in flight; results keep
/// the order of `items`, and the first failure drops the unfinished tasks.
async fn ordered_concurrently<I, T, Fut>(
    items: Vec<I>,
    limit: usize,
    task: impl FnMut(I) -> Fut,
) -> Result<Vec<T>, LspFailure>
where
    Fut: std::future::Future<Output = Result<T, LspFailure>>,
{
    stream::iter(items)
        .map(task)
        .buffered(limit)
        .try_collect()
        .await
}

/// Candidate files verified at once on a server that pipelines requests.
const CONCURRENT_FILE_VERIFICATIONS: usize = 8;

/// What the definition chains of one candidate's occurrences proved.
struct FileVerdict {
    file: String,
    anchors: Vec<Anchor>,
    /// A chain failed or stopped at an import the server did not follow, so
    /// without an anchor the file stays unverified rather than rejected.
    unresolved: bool,
}

async fn verify_file(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    (file, spots): OpenedFile,
    declaration: &HashSet<String>,
    missing: &(impl Fn(&Path) -> bool + Sync),
) -> Result<FileVerdict, LspFailure> {
    let content = match sources.get(&file).await {
        Some(source) => source.content.clone(),
        None => String::new(),
    };
    let mut anchors = Vec::new();
    let mut unresolved = false;
    // One verified anchor answers for the whole file's program; keep
    // checking only until a verified call site is found as well.
    let mut verified_any = false;
    for (spot_line, spot_character, is_call) in spots {
        if verified_any && !is_call {
            continue;
        }
        cancel.check().map_err(LspFailure::cancelled)?;
        let resolved = identities(
            client,
            sources,
            snippet_policy,
            cancel,
            &file,
            spot_line,
            spot_character,
        )
        .await?;
        let Some(resolved) = resolved else {
            unresolved = true;
            continue;
        };
        if !resolved.is_disjoint(declaration) {
            anchors.push(Anchor {
                path: file.clone(),
                line: spot_line,
                character: spot_character,
                is_call,
            });
            if is_call {
                break;
            }
            verified_any = true;
        } else if stays_on_import(&resolved, &file, declaration, &content, missing) {
            unresolved = true;
        }
    }
    Ok(FileVerdict {
        file,
        anchors,
        unresolved,
    })
}

/// Declaration identities reached by the definition chain at a position.
async fn identities(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    path: &str,
    line: u32,
    character: u32,
) -> Result<Option<HashSet<String>>, LspFailure> {
    match resolve_definition_chain(
        client,
        sources,
        snippet_policy,
        cancel,
        path,
        line,
        character,
    )
    .await
    {
        Ok((found, warnings)) if warnings.is_empty() => {
            Ok(Some(found.iter().map(snippet_identity).collect()))
        }
        // A retained pre-failure alias is not terminal declaration proof.
        Ok(_) => Ok(None),
        Err(failure) if failure.is_cancelled() => Err(failure),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::super::blocking_cancellable;
    use super::*;
    use octocode_contract_types::ErrorCode;

    #[test]
    fn occurrences_are_word_bounded_and_utf16() {
        let content = "const { stage } = require('./a');\nstaged(stage);\n// é stage\n";
        assert_eq!(
            occurrences(content, "stage"),
            vec![(0, 8, false), (1, 7, false), (2, 5, false)]
        );
        assert_eq!(occurrences("stage (1)", "stage"), vec![(0, 0, true)]);
    }

    #[test]
    fn occurrences_include_late_call_sites() {
        let content = "// f f f f f\nf();\n";
        let found = occurrences(content, "f");
        assert_eq!(found.len(), 6);
        assert_eq!(found.last(), Some(&(1, 0, true)));
    }

    #[test]
    fn failed_importer_verification_never_reports_complete_coverage() {
        let importers = Importers {
            failed: true,
            ..Importers::default()
        };
        let mut row = json!({"status": "success"});
        let query = serde_json::from_value(json!({
            "operation": "references", "path": "/r/a.ts", "symbolName": "x", "line": 1
        }))
        .expect("query");
        importers.annotate(&mut row, &query, &Scope::new("/r".into(), Vec::new()));
        assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED);
    }

    /// Unverified candidates stay out of the answer, so `textOnlyFiles` and
    /// the lexical lead disclose them, and the scan is never complete.
    #[test]
    fn unverified_candidates_are_not_answered_and_fail_the_scan() {
        let list = candidates(3);
        let Cut::Window(window) = Window::cut(list.clone(), WindowRequest::default()) else {
            panic!("window 1 is never stale");
        };
        let importers = Importers {
            window: Some(window),
            failed: true,
            unverified: vec![list[1].clone()],
            ..Importers::default()
        };
        assert_eq!(importers.covered(), vec![list[0].clone(), list[2].clone()]);
        let scope = Scope::new("/r".into(), Vec::new());
        let query = serde_json::from_value(json!({
            "operation": "references", "path": "/r/a.ts", "symbolName": "x", "line": 1
        }))
        .expect("query");
        let mut row = json!({"status": "success"});
        importers.annotate(&mut row, &query, &scope);
        assert_eq!(row["coverage"]["importerScan"], SCAN_FAILED);
    }

    /// A gitignored file that imports the name, one the check could not
    /// decide, or a scan that stopped at its bound makes the row partial;
    /// files that only spell it and nested repositories leave a lead and no
    /// partial reason.
    #[test]
    fn ignored_importers_or_an_unfinished_scan_make_the_row_partial() {
        let query: LspSearchQuery = serde_json::from_value(json!({
            "operation": "references", "path": "/r/a.ts", "symbolName": "x", "line": 1
        }))
        .expect("query");
        let scope = Scope::new("/r".into(), vec!["*.ts".into()]);
        let annotate = |ignored: IgnoredMentions| {
            let importers = Importers {
                ignored,
                ..Importers::default()
            };
            let mut row = json!({"status": "success"});
            importers.annotate(&mut row, &query, &scope);
            row
        };
        let quiet = annotate(IgnoredMentions::default());
        assert!(quiet.get("isPartial").is_none(), "{quiet}");

        // Nested repositories the scan did not enter are counted, whatever
        // else it found; the lead names each one.
        let nested = annotate(IgnoredMentions {
            nested_repositories: vec!["repos/a".into(), "repos/b".into()],
            ..IgnoredMentions::default()
        });
        assert_eq!(
            nested[crate::response::paging::NOTICES],
            json!([
                "The scan for gitignored files did not enter 2 nested repositories (directories with their own .git) under the scope root."
            ]),
            "{nested}"
        );
        assert!(quiet["next"].get("includeSkipped").is_none(), "{quiet}");
        // They are no evidence, so the row stays whole, but the lead reaches
        // into each of them.
        assert!(nested.get("isPartial").is_none(), "{nested}");
        assert!(nested.get("partialReasons").is_none(), "{nested}");
        let lead_paths = |row: &Value| {
            row["next"]["includeSkipped"]["query"]["queries"]
                .as_array()
                .expect("leads")
                .iter()
                .map(|lead| {
                    assert_eq!(lead["includeIgnored"], true, "{row}");
                    lead["path"].clone()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            lead_paths(&nested),
            [json!("/r/repos/a"), json!("/r/repos/b")],
            "{nested}"
        );
        // With evidence, one lead covers it and the nested repositories.
        let nested_hidden = annotate(IgnoredMentions {
            files: vec!["/r/gen/g.ts".into()],
            importing: vec!["/r/gen/g.ts".into()],
            nested_repositories: vec!["repos/a".into()],
            ..IgnoredMentions::default()
        });
        assert_eq!(
            lead_paths(&nested_hidden),
            [json!("/r/gen/g.ts"), json!("/r/repos/a")],
            "{nested_hidden}"
        );

        // An unfinished scan left importers unverified: a partial row, a
        // lead over the scope, and a note.
        let unfinished = annotate(IgnoredMentions::unfinished());
        assert_eq!(unfinished["isPartial"], true, "{unfinished}");
        assert_eq!(
            unfinished["partialReasons"],
            json!([IGNORED_REASON]),
            "{unfinished}"
        );
        let lead = &unfinished["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(lead["path"], "/r", "{unfinished}");
        assert_eq!(lead["includeIgnored"], true, "{unfinished}");
        assert_eq!(
            unfinished[crate::response::paging::NOTICES],
            json!([IGNORED_UNFINISHED_NOTICE]),
            "{unfinished}"
        );

        // Bundled copies that only spell the name: their exact lead.
        let copies = annotate(IgnoredMentions {
            files: vec!["/r/releases/copy.mjs".into()],
            rules: vec![".gitignore:1 'releases/**'".into()],
            ..IgnoredMentions::default()
        });
        assert!(copies.get("isPartial").is_none(), "{copies}");
        assert_eq!(
            copies["next"]["includeSkipped"]["query"]["queries"][0]["path"], "/r/releases/copy.mjs",
            "{copies}"
        );

        // An ignored importer: partial, named with its rule, lead to it.
        let hidden = annotate(IgnoredMentions {
            files: vec!["/r/gen/g.ts".into(), "/r/gen/h.ts".into()],
            rules: vec![".gitignore:1 'gen/'".into()],
            unfinished: true,
            importing: vec!["/r/gen/g.ts".into()],
            ..IgnoredMentions::default()
        });
        assert_eq!(
            hidden["partialReasons"],
            json!([IGNORED_REASON]),
            "{hidden}"
        );
        let warning = hidden["warnings"][0].as_str().expect("warning");
        assert!(
            warning.contains("1 imports it")
                && !warning.contains("could not be checked")
                && warning.contains("more may exist"),
            "{hidden}"
        );
        // More may exist past where the scan stopped: the lead names the
        // importer it found, then covers the scope for what it did not reach.
        assert_eq!(
            lead_paths(&hidden),
            [json!("/r/gen/g.ts"), json!("/r")],
            "{hidden}"
        );
        let past_stop = &hidden["next"]["includeSkipped"]["query"]["queries"][1];
        assert_eq!(past_stop["include"], json!(["*.ts"]), "{hidden}");
        // Undecided files and nested repositories are named the same way.
        let hidden_unchecked = annotate(IgnoredMentions {
            files: vec!["/r/gen/big.js".into(), "/r/gen/g.ts".into()],
            unfinished: true,
            importing: vec!["/r/gen/g.ts".into()],
            unchecked: vec!["/r/gen/big.js".into()],
            nested_repositories: vec!["repos/a".into()],
            ..IgnoredMentions::default()
        });
        assert_eq!(
            lead_paths(&hidden_unchecked),
            [
                json!("/r/gen/g.ts"),
                json!("/r/gen/big.js"),
                json!("/r/repos/a"),
                json!("/r")
            ],
            "{hidden_unchecked}"
        );

        // Files the check could not decide are counted apart, never as
        // importers, and still lead.
        let undecided = annotate(IgnoredMentions {
            files: vec!["/r/gen/big.js".into(), "/r/gen/g.ts".into()],
            rules: vec![".gitignore:1 'gen/'".into()],
            importing: vec!["/r/gen/g.ts".into()],
            unchecked: vec!["/r/gen/big.js".into()],
            ..IgnoredMentions::default()
        });
        assert_eq!(undecided["partialReasons"], json!([IGNORED_REASON]));
        let warning = undecided["warnings"][0].as_str().expect("warning");
        assert!(
            warning.contains("1 imports it and 1 could not be checked"),
            "{undecided}"
        );
        let leads = undecided["next"]["includeSkipped"]["query"]["queries"]
            .as_array()
            .expect("leads")
            .iter()
            .map(|lead| lead["path"].clone())
            .collect::<Vec<_>>();
        assert_eq!(leads, [json!("/r/gen/g.ts"), json!("/r/gen/big.js")]);
    }

    /// The import check reads each ignored file through the bounded reader:
    /// a file too large or unparsable, and every file past the check bound,
    /// stays unchecked rather than counting as an importer.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ignored_files_the_check_cannot_decide_stay_unchecked() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("canonical");
        let write = |name: &str, text: &str| {
            let path = root.join(name);
            std::fs::write(&path, text).expect("write");
            path.to_string_lossy().into_owned()
        };
        let importer = write("imp.ts", "import { x } from './a';\nx();\n");
        let copy = write("copy.ts", "export function x() {}\n");
        let large = write("large.js", &"// x\n".repeat(CODE_CHECK_MAX_BYTES / 5 + 1));
        let mut files = vec![importer.clone(), copy, large.clone()];
        let past = (0..CODE_CHECK_MAX_FILES)
            .map(|index| write(&format!("p{index}.ts"), "import { x } from './a';\n"))
            .collect::<Vec<_>>();
        files.extend(past.iter().cloned());
        let mut ignored = IgnoredMentions {
            files,
            ..IgnoredMentions::default()
        };
        let policy = crate::tools::test_support::workspace_policy(&root);
        check_imports(
            &mut ignored,
            &policy,
            &crate::execution::cancel::NeverCancel,
            "x",
        )
        .await
        .expect("check");
        // The first CODE_CHECK_MAX_FILES are read: the importer and all but 3 of
        // the past files import.
        assert_eq!(ignored.importing.len(), 1 + CODE_CHECK_MAX_FILES - 3);
        assert_eq!(ignored.importing[0], importer);
        let mut unchecked = vec![large];
        unchecked.extend(past[CODE_CHECK_MAX_FILES - 3..].iter().cloned());
        assert_eq!(ignored.unchecked, unchecked);
    }

    #[test]
    fn importing_is_by_name_or_namespace_member() {
        let imports = |content: &str, file: &str| imports_name(content, file, "x");
        assert_eq!(
            imports("import { x } from './a';\nx();\n", "a.ts"),
            Some(true)
        );
        assert_eq!(
            imports("import { x as y } from './a';\n", "a.ts"),
            Some(true)
        );
        assert_eq!(
            imports("import * as a from './a';\na.x();\n", "a.ts"),
            Some(true)
        );
        assert_eq!(
            imports("import * as a from './a';\na.xy();\n", "a.ts"),
            Some(false)
        );
        // A bundled copy declaring its own `x` imports nothing of it.
        assert_eq!(
            imports(
                "import fs from 'node:fs';\nexport function x() { return fs; }\n",
                "copy.mjs"
            ),
            Some(false)
        );
    }

    #[test]
    fn call_sites_prefer_calls_one_per_file() {
        let anchor = |path: &str, line, is_call| Anchor {
            path: path.to_owned(),
            line,
            character: 0,
            is_call,
        };
        let importers = Importers {
            anchors: vec![
                anchor("b", 0, false),
                anchor("b", 2, true),
                anchor("c", 1, false),
            ],
            ..Importers::default()
        };
        let sites = importers
            .call_sites()
            .iter()
            .map(|a| (a.path.as_str(), a.line))
            .collect::<Vec<_>>();
        assert_eq!(sites, vec![("b", 2), ("c", 1)]);
        assert_eq!(importers.per_file().count(), 2);
    }

    /// Cancels once `flag` is set, or once `deadline` passes (as a timeout).
    struct LiveCancel {
        flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        deadline: Option<std::time::Instant>,
    }
    impl CancellationCheck for LiveCancel {
        fn check(&self) -> Result<(), String> {
            if self
                .deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
            {
                return Err("Timeout".into());
            }
            if self.flag.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("Cancelled".into());
            }
            Ok(())
        }
    }

    /// A worker that reports when it starts, then runs until its stop
    /// callback fires (or a safety bound passes), and reports whether it
    /// stopped because of the callback.
    /// The worker finished and reported `true` within 5 s.
    fn assert_worker_stopped(finished: &std::sync::mpsc::Receiver<bool>) {
        assert!(
            finished
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("worker terminated")
        );
    }

    fn observed_worker(
        started: std::sync::mpsc::Sender<()>,
        finished: std::sync::mpsc::Sender<bool>,
    ) -> impl FnOnce(&(dyn Fn() -> bool + Sync)) + Send + 'static {
        move |stopped| {
            let _ = started.send(());
            let bound = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !stopped() && std::time::Instant::now() < bound {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let _ = finished.send(stopped());
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_running_worker_stops_when_the_request_is_cancelled_after_launch() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel = LiveCancel {
            flag: flag.clone(),
            deadline: None,
        };
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let canceller = std::thread::spawn(move || {
            started_rx
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("worker started");
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let failure = blocking_cancellable(&cancel, observed_worker(started_tx, finished_tx))
            .await
            .expect_err("cancelled after launch");
        assert_eq!(failure.code, ErrorCode::Cancelled);
        assert_eq!(failure.message, "Cancelled");
        canceller.join().expect("canceller");
        assert!(
            finished_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("worker terminated"),
            "the worker must stop through its stop callback"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_deadline_expiring_during_the_scan_stops_the_worker_as_a_timeout() {
        let cancel = LiveCancel {
            flag: std::sync::Arc::default(),
            deadline: Some(std::time::Instant::now() + std::time::Duration::from_millis(150)),
        };
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let failure = blocking_cancellable(&cancel, observed_worker(started_tx, finished_tx))
            .await
            .expect_err("deadline expired");
        started_rx.try_recv().expect("worker had started");
        assert_eq!(failure.code, ErrorCode::Cancelled);
        assert_eq!(failure.message, "Timeout");
        assert_worker_stopped(&finished_rx);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_the_request_stops_the_worker() {
        let (started_tx, _started_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let dropped = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            blocking_cancellable(
                &crate::execution::cancel::NeverCancel,
                observed_worker(started_tx, finished_tx),
            ),
        )
        .await;
        assert!(dropped.is_err(), "request future dropped by the timeout");
        assert_worker_stopped(&finished_rx);
    }

    /// The real importer scan: cancellation observed after the request
    /// started is a cancellation, never a completed candidate list.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_candidate_scan_reports_cancellation_after_launch() {
        struct CancelAfterFirst(std::sync::atomic::AtomicUsize);
        impl CancellationCheck for CancelAfterFirst {
            fn check(&self) -> Result<(), String> {
                if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    Ok(())
                } else {
                    Err("Cancelled".into())
                }
            }
        }
        let root = tempfile::tempdir().expect("fixture");
        for index in 0..200 {
            std::fs::write(
                root.path().join(format!("f{index}.ts")),
                "import { target } from './a';\ntarget();\n",
            )
            .expect("fixture file");
        }
        let root_str = root.path().to_string_lossy().into_owned();
        let policy = |workspace: &Path| crate::tools::test_support::workspace_policy(workspace);
        let scope = Scope::new(root_str.clone(), vec!["*.ts".into()]);
        let failure = candidate_files(
            &scope,
            "target",
            &HashSet::new(),
            &policy(root.path()),
            &CancelAfterFirst(std::sync::atomic::AtomicUsize::new(0)),
        )
        .await
        .expect_err("cancellation is not an empty or complete scan");
        assert_eq!(failure.code, ErrorCode::Cancelled);
        let scope = Scope::new(root_str, vec!["*.ts".into()]);
        let complete = candidate_files(
            &scope,
            "target",
            &HashSet::new(),
            &policy(root.path()),
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan")
        .expect("scan succeeded");
        assert_eq!(complete.len(), 200, "every candidate, uncut");
        let Cut::Window(window) = Window::cut(complete, WindowRequest::default()) else {
            panic!("first window");
        };
        assert_eq!(window.files().len(), MAX_CANDIDATE_FILES);
        assert!(window.more(), "more candidates than one window");
    }

    fn candidates(count: usize) -> Vec<String> {
        (0..count)
            .map(|index| format!("/r/f{index:03}.ts"))
            .collect()
    }

    /// Following each window's digest from window 1 verifies every
    /// candidate in exactly one window, in order, with no gap.
    #[test]
    fn importer_windows_cover_every_candidate_exactly_once() {
        for count in [0, 1, 23, 24, 25, 48, 60, 73] {
            let list = candidates(count);
            let mut reached: Vec<String> = Vec::new();
            let (mut page, mut digest) = (1, None::<String>);
            loop {
                let request = WindowRequest {
                    page,
                    expected: digest.as_deref(),
                };
                let Cut::Window(window) = Window::cut(list.clone(), request) else {
                    panic!("window {page} of {count} is stale");
                };
                assert!(window.files().len() <= MAX_CANDIDATE_FILES);
                assert_eq!(window.covered().len(), reached.len() + window.files().len());
                assert_eq!(
                    window.foreign().count() + window.files().len(),
                    count,
                    "own and foreign windows partition the list"
                );
                for file in window.files() {
                    assert_eq!(window.owner(file), page, "{file} belongs to its window");
                }
                assert_eq!(window.owner("/elsewhere.ts"), page, "non-candidates stay");
                reached.extend(window.files().iter().cloned());
                if !window.more() {
                    assert_eq!(window.pages, page);
                    break;
                }
                page += 1;
                digest = Some(window.digest.clone());
            }
            assert_eq!(reached, list, "{count} candidates: once each, in order");
            assert_eq!(page as usize, count.div_ceil(MAX_CANDIDATE_FILES).max(1));
        }
    }

    /// A later window whose candidate list changed (or that no longer
    /// exists) is stale; the first window never is.
    #[test]
    fn a_changed_candidate_list_stales_a_later_window() {
        let Cut::Window(first) = Window::cut(candidates(60), WindowRequest::default()) else {
            panic!("first window");
        };
        let mut changed = candidates(60);
        changed.insert(5, "/r/f004a.ts".into());
        let entering = WindowRequest {
            page: 2,
            expected: Some(&first.digest),
        };
        assert!(matches!(Window::cut(changed.clone(), entering), Cut::Stale));
        assert!(matches!(
            Window::cut(candidates(60), entering),
            Cut::Window(_)
        ));
        // A missing snapshot is not a match.
        let unsnapshotted = WindowRequest {
            page: 2,
            expected: Some(""),
        };
        assert!(matches!(
            Window::cut(candidates(60), unsnapshotted),
            Cut::Stale
        ));
        // Past the last window.
        let past = WindowRequest {
            page: 4,
            expected: None,
        };
        assert!(matches!(Window::cut(candidates(60), past), Cut::Stale));
        assert!(matches!(
            Window::cut(changed, WindowRequest::default()),
            Cut::Window(_)
        ));
    }

    /// Importer verification on a pipelining server: results keep candidate
    /// order, no more than `limit` run at once, and several really overlap.
    #[tokio::test]
    async fn bounded_verification_overlaps_tasks_and_keeps_order() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (running, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let done = ordered_concurrently((0..12u64).collect(), 4, |item| {
            let (running, peak) = (&running, &peak);
            async move {
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                // Later items finish first, so order comes from the combinator.
                tokio::time::sleep(std::time::Duration::from_millis(40 - item * 3)).await;
                running.fetch_sub(1, Ordering::SeqCst);
                Ok(item)
            }
        })
        .await
        .expect("all tasks succeed");
        assert_eq!(done, (0..12).collect::<Vec<_>>());
        assert_eq!(peak.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn a_failed_verification_stops_the_rest() {
        let started = std::sync::atomic::AtomicUsize::new(0);
        let result = ordered_concurrently((0..20u32).collect(), 2, |item| {
            let started = &started;
            async move {
                started.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if item == 1 {
                    Err(LspFailure::invalid_query("boom"))
                } else {
                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                    Ok(item)
                }
            }
        })
        .await;
        assert!(result.is_err());
        assert!(started.load(std::sync::atomic::Ordering::SeqCst) < 20);
    }

    #[test]
    fn applies_only_to_ts_js_incoming_operations() {
        assert!(applies(Some("javascript"), "references"));
        assert!(applies(Some("typescript"), "callers"));
        assert!(!applies(Some("rust"), "references"));
        assert!(!applies(Some("typescript"), "definition"));
        assert!(!applies(None, "references"));
    }
}
