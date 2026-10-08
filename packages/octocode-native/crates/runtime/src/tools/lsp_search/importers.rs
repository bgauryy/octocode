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
use super::failure::{LspFailure, flag_partial, query_value};
use super::recovery::{get_locations, resolve_definition_chain, snippet_identity};
use super::render::{TS_LANGUAGE_IDS, uri_to_path};
use super::scope::{Scope, canonical};
use super::source::SourceCache;
use crate::policy::path::PathPolicy;
use crate::tools::cancel::CancellationCheck;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;
/// Candidate files opened and verified per request: one importer window.
pub(super) const MAX_CANDIDATE_FILES: usize = 24;
/// Prefix of a candidate-list digest (the `snapshot` of
/// `next.nextImporterPage`).
const DIGEST_PREFIX: &str = "lsp-imp:";
/// Partial reason of a window that leaves later windows unverified.
pub(super) const CAPPED_REASON: &str = "importerScanCapped";
/// Settle/ready bounds for the last candidate open; the load it triggers
/// covers every candidate opened before it.
const OPEN_SETTLE_MS: u32 = 200;
const OPEN_READY_TIMEOUT_MS: u32 = 10_000;

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
        let page = query.importer_page();
        // A later location page carries its location snapshot instead; that
        // snapshot is salted with the candidate digest (see [`Window::digest`]).
        let entering = page > 1 && query.page().unwrap_or(1) == 1;
        Self {
            page,
            expected: entering.then(|| query.snapshot().unwrap_or_default()),
        }
    }
}

/// One importer window of the sorted candidate list.
#[derive(Debug)]
pub(super) struct Window {
    /// One-based window index and window count.
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
            crate::digest::sha256(candidates.join("\u{0}").as_bytes())
        );
        let total = candidates.len();
        let pages = u32::try_from(total.div_ceil(MAX_CANDIDATE_FILES).max(1)).unwrap_or(u32::MAX);
        let page = request.page.max(1);
        if page > pages || request.expected.is_some_and(|expected| expected != digest) {
            return Cut::Stale;
        }
        Cut::Window(Self {
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

    /// This window's candidates.
    pub(super) fn files(&self) -> &[String] {
        &self.candidates[self.range(self.page)]
    }

    /// Candidates of this window and every earlier one: the files the walk
    /// has verified once this page is read.
    pub(super) fn covered(&self) -> &[String] {
        &self.candidates[..self.range(self.page).end]
    }

    /// Candidates of every other window.
    pub(super) fn foreign(&self) -> impl Iterator<Item = &String> {
        let own = self.range(self.page);
        self.candidates[..own.start]
            .iter()
            .chain(&self.candidates[own.end..])
    }

    /// The window that owns a file (canonical path): its candidate window,
    /// or this one for a file outside the list.
    pub(super) fn owner(&self, file: &str) -> u32 {
        self.candidates
            .binary_search_by(|candidate| candidate.as_str().cmp(file))
            .map_or(self.page, |index| {
                u32::try_from(index / MAX_CANDIDATE_FILES + 1).unwrap_or(u32::MAX)
            })
    }

    /// Whether a later window remains.
    pub(super) fn more(&self) -> bool {
        self.page < self.pages
    }
}

/// True for operations whose answer lists places that point *at* the anchor.
pub(super) fn applies(language_id: Option<&str>, operation: &str) -> bool {
    language_id.is_some_and(|id| TS_LANGUAGE_IDS.contains(&id))
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

    /// Whether a recovered location in `file` (canonical) belongs to this
    /// request's window: a candidate's locations belong to its own window.
    pub(super) fn owns(&self, file: &str) -> bool {
        self.window
            .as_ref()
            .is_none_or(|window| window.owner(file) == window.page)
    }

    /// The candidate digest, salted into this request's location snapshot so
    /// a changed candidate list stales the window's later location pages.
    pub(super) fn digest(&self) -> Option<&str> {
        self.window.as_ref().map(|window| window.digest.as_str())
    }

    /// Candidates verified by this window and every earlier one.
    pub(super) fn covered(&self) -> &[String] {
        self.window.as_ref().map_or(&[], Window::covered)
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
        if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
            let coverage = payload
                .entry("coverage")
                .or_insert_with(|| json!({"scope":"languageServer","exhaustive":false}));
            coverage["importerScan"] = json!(if self.failed {
                SCAN_FAILED
            } else if more {
                SCAN_CAPPED
            } else {
                SCAN_COMPLETE
            });
            coverage["verifiedImporterFiles"] = json!(files);
        }
        let Some(window) = self.window.as_ref().filter(|window| window.more()) else {
            return;
        };
        let range = window.range(window.page + 1);
        let last_location_page =
            row.pointer("/pagination/hasMore").and_then(Value::as_bool) != Some(true);
        if !self.failed {
            let warning = format!(
                "{} candidate files mention this name outside the server's answer; importer recovery verifies {MAX_CANDIDATE_FILES} per importer page. This is importer page {} of {}; {}next.nextImporterPage verifies candidates {}-{}.",
                window.total,
                window.page,
                window.pages,
                if last_location_page {
                    ""
                } else {
                    "after this window's last location page (next.nextPage), "
                },
                range.start + 1,
                range.end,
            );
            flag_partial(row, query, CAPPED_REASON, &warning, scope);
        }
        if !last_location_page {
            return;
        }
        let mut next = query_value(query);
        if let Some(fields) = next.as_object_mut() {
            fields.remove("page");
            fields.insert("importerPage".into(), json!(window.page + 1));
            fields.insert("snapshot".into(), json!(window.digest));
        }
        row["next"]["nextImporterPage"] =
            crate::tools::result::Continuation::new(crate::tools::id::ToolId::LspSearch, next)
                .why(format!(
                    "Verify importer candidates {}-{} of {} (importer page {} of {}).",
                    range.start + 1,
                    range.end,
                    window.total,
                    window.page + 1,
                    window.pages
                ))
                .confidence("exact")
                .build();
    }
}

pub(super) const SCAN_COMPLETE: &str = "complete";
pub(super) const SCAN_CAPPED: &str = "capped";
pub(super) const SCAN_FAILED: &str = "failed";

/// Verified importer anchors for `symbol` declared at the request anchor,
/// from the importer window `request` names. `known_files` are files the
/// server already reported; they are not candidates.
#[allow(clippy::too_many_arguments)]
pub(super) async fn verified_anchors(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    symbol: &str,
    scope: &Scope,
    anchor_path: &str,
    line: u32,
    character: u32,
    known_files: &HashSet<String>,
    request: WindowRequest<'_>,
) -> Result<Importers, LspFailure> {
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
            ..Importers::default()
        });
    };
    let (opened, open_failed) =
        open_candidates(client, sources, cancel, window.files(), symbol).await?;
    let (anchors, rejected, verify_failed) = verify_occurrences(
        client,
        sources,
        snippet_policy,
        cancel,
        opened,
        &declaration,
    )
    .await?;
    let failed = open_failed || verify_failed;
    Ok(Importers {
        anchors,
        window: Some(window),
        stale: false,
        failed,
        rejected,
    })
}

/// A synchronized candidate and its `(line, character, is-call)` occurrences.
type OpenedFile = (String, Vec<(u32, u32, bool)>);

/// Open every candidate that mentions `symbol`; wait once, on the last, for
/// the project load the opens trigger, so identity checks do not race it.
/// Returns each opened file's occurrences and whether any candidate failed.
async fn open_candidates(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    files: &[String],
    symbol: &str,
) -> Result<(Vec<OpenedFile>, bool), LspFailure> {
    let mut opened = Vec::new();
    let mut failed = false;
    for (index, file) in files.iter().enumerate() {
        cancel.check().map_err(LspFailure::cancelled)?;
        let Some(source) = sources.get(file).await else {
            failed = true;
            continue;
        };
        let spots = occurrences(&source.content, symbol);
        if spots.is_empty() {
            continue;
        }
        let synced = if index + 1 == files.len() {
            client
                .open_document_and_wait(
                    file.clone(),
                    &source.content,
                    Some(OPEN_SETTLE_MS),
                    Some(OPEN_READY_TIMEOUT_MS),
                )
                .await
                .map(|_| ())
        } else {
            client.open_document(file.clone(), &source.content).await
        };
        if synced.is_ok() {
            opened.push((file.clone(), spots));
        } else {
            failed = true;
        }
    }
    Ok((opened, failed))
}

/// Keep each occurrence whose definition chain reaches `declaration`.
/// Returns the verified anchors, the files whose every occurrence resolved
/// elsewhere, and whether any identity check failed.
async fn verify_occurrences(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    opened: Vec<OpenedFile>,
    declaration: &HashSet<String>,
) -> Result<(Vec<Anchor>, Vec<String>, bool), LspFailure> {
    let mut failed = false;
    let mut anchors = Vec::new();
    let mut rejected = Vec::new();
    for (file, spots) in opened {
        let mut file_failed = false;
        let verified_before = anchors.len();
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
                failed = true;
                file_failed = true;
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
            }
        }
        if !file_failed && anchors.len() == verified_before {
            rejected.push(file);
        }
    }
    Ok((anchors, rejected, failed))
}

/// LSP `SymbolKind`s that own call sites: method, constructor, function.
const CALLABLE_KINDS: [u64; 3] = [6, 9, 12];
/// LSP `SymbolKind::File`, for calls made at module top level.
const FILE_KIND: u64 = 1;

/// Caller edges `(node, call sites)` derived from each verified importer's
/// references, for servers whose call hierarchy cannot cross the importer's
/// binding (CommonJS `module.exports = { f }` + destructured `require`).
/// A reference is a call when `(` follows it; its caller is the innermost
/// function or method the server's `documentSymbol` places around it, or
/// the file itself for a top-level call. Files the walk's own call
/// hierarchy already answered (`answered`, canonical paths) are skipped
/// before any request.
pub(super) async fn callers_from_references(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    importers: &Importers,
    answered: &HashSet<String>,
) -> Result<Vec<(Value, Vec<Value>)>, LspFailure> {
    let mut edges: Vec<(Value, Vec<Value>)> = Vec::new();
    for anchor in importers.per_file() {
        cancel.check().map_err(LspFailure::cancelled)?;
        if answered.contains(&canonical(&anchor.path)) {
            continue;
        }
        let file = canonical(&anchor.path);
        // Only the importer's own sites are kept: read no other file's
        // snippet (a references answer names every file using the symbol).
        let own_file = own_file_policy(sources.policy(), &file);
        let Ok(references) = get_locations(
            client,
            &own_file,
            cancel,
            LocationRequest::References {
                include_declaration: false,
            },
            &anchor.path,
            anchor.line,
            anchor.character,
        )
        .await
        else {
            continue;
        };
        let Some(source) = sources.get(&anchor.path).await else {
            continue;
        };
        let calls = references
            .iter()
            .filter(|snippet| canonical(&uri_to_path(&snippet.uri)) == file)
            .filter(|snippet| {
                is_call_at(
                    &source.content,
                    snippet.range.end.line,
                    snippet.range.end.character,
                )
            })
            .collect::<Vec<_>>();
        if calls.is_empty() {
            continue;
        }
        let symbols = client
            .get_document_symbols(anchor.path.clone())
            .await
            .unwrap_or_default();
        let mut flat = Vec::new();
        flatten_symbols(&symbols, &mut flat);
        for call in calls {
            let site = json!({
                "start": {"line": call.range.start.line, "character": call.range.start.character},
                "end": {"line": call.range.end.line, "character": call.range.end.character}
            });
            let node = enclosing_callable(&flat, call.range.start.line, call.range.start.character)
                .map(|symbol| {
                    json!({
                        "name": symbol["name"],
                        "kind": symbol["kind"],
                        "uri": call.uri,
                        "range": symbol["range"],
                        "selectionRange": symbol["selectionRange"],
                    })
                })
                .unwrap_or_else(|| {
                    let name = Path::new(&file)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let origin = json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}});
                    json!({"name": name, "kind": FILE_KIND, "uri": call.uri, "range": origin, "selectionRange": origin})
                });
            match edges.iter_mut().find(|(known, _)| {
                known["uri"] == node["uri"] && known["selectionRange"] == node["selectionRange"]
            }) {
                Some((_, sites)) => sites.push(site),
                None => edges.push((node, vec![site])),
            }
        }
    }
    Ok(edges)
}

/// A snippet policy that reads only `file` (canonical) under `paths`;
/// locations in other files keep their range with withheld content.
fn own_file_policy(paths: &PathPolicy, file: &str) -> SnippetReadPolicy {
    let (paths, file) = (paths.clone(), file.to_owned());
    SnippetReadPolicy::with_authorizer(move |path| {
        // A reference answer names every file using the symbol: refuse the
        // others by their (memoized) canonical form before any validation.
        if canonical(&path.to_string_lossy()) != file {
            return None;
        }
        let valid = paths.validate_read(path).ok()?.canonical;
        (valid.to_string_lossy() == file).then_some(valid)
    })
}

/// True when `(` (after optional spaces) follows the UTF-16 position.
fn is_call_at(content: &str, line: u32, character: u32) -> bool {
    let Some(text) = usize::try_from(line)
        .ok()
        .and_then(|line| content.lines().nth(line))
    else {
        return false;
    };
    let mut units = 0u32;
    let mut at = text.len();
    for (index, c) in text.char_indices() {
        if units >= character {
            at = index;
            break;
        }
        units += u32::try_from(c.len_utf16()).unwrap_or(2);
    }
    text[at..].trim_start().starts_with('(')
}

/// One symbol of a flattened `documentSymbol` answer, borrowed from it,
/// with its range points read once. Indexing by `name`, `kind`, `range`,
/// or `selectionRange` (defaults to `range`) gives the answer's field.
pub(super) struct FlatSymbol<'a> {
    name: &'a Value,
    kind: &'a Value,
    range: &'a Value,
    selection: &'a Value,
    start: (u64, u64),
    end: (u64, u64),
}

impl std::ops::Index<&str> for FlatSymbol<'_> {
    type Output = Value;

    fn index(&self, field: &str) -> &Value {
        match field {
            "name" => self.name,
            "kind" => self.kind,
            "range" => self.range,
            "selectionRange" => self.selection,
            _ => &Value::Null,
        }
    }
}

/// DocumentSymbol trees (and flat SymbolInformation lists) as one flat list
/// of borrowed symbols, in one pass.
pub(super) fn flatten_symbols<'a>(value: &'a Value, out: &mut Vec<FlatSymbol<'a>>) {
    let point = |range: &Value, edge: &str| {
        let at = |field: &str| {
            range
                .get(edge)
                .and_then(|point| point.get(field))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        (at("line"), at("character"))
    };
    for symbol in value.as_array().into_iter().flatten() {
        if let Some(range) = symbol
            .get("range")
            .or_else(|| symbol.pointer("/location/range"))
        {
            out.push(FlatSymbol {
                name: symbol.get("name").unwrap_or(&Value::Null),
                kind: symbol.get("kind").unwrap_or(&Value::Null),
                range,
                selection: symbol
                    .get("selectionRange")
                    .filter(|selection| !selection.is_null())
                    .unwrap_or(range),
                start: point(range, "start"),
                end: point(range, "end"),
            });
        }
        if let Some(children) = symbol.get("children") {
            flatten_symbols(children, out);
        }
    }
}

/// Innermost callable symbol whose range contains the position.
pub(super) fn enclosing_callable<'s, 'a>(
    symbols: &'s [FlatSymbol<'a>],
    line: u32,
    character: u32,
) -> Option<&'s FlatSymbol<'a>> {
    let at = (u64::from(line), u64::from(character));
    symbols
        .iter()
        .filter(|symbol| {
            symbol
                .kind
                .as_u64()
                .is_some_and(|kind| CALLABLE_KINDS.contains(&kind))
        })
        .filter(|symbol| symbol.start <= at && at <= symbol.end)
        .max_by_key(|symbol| symbol.start)
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
        Err(failure) if failure.code == "cancelled" => Err(failure),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::super::blocking_cancellable;
    use super::*;

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
        let mut row = json!({"status": "success", "payload": {}});
        let query = serde_json::from_value(json!({
            "operation": "references", "path": "/r/a.ts", "symbolName": "x", "lineHint": 1
        }))
        .expect("query");
        importers.annotate(&mut row, &query, &Scope::new("/r".into(), Vec::new()));
        assert_eq!(row["payload"]["coverage"]["importerScan"], SCAN_FAILED);
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

    #[test]
    fn call_sites_and_enclosing_callables() {
        assert!(is_call_at("  return stage (1);", 0, 14));
        assert!(!is_call_at("const { stage } = x;", 0, 13));
        let tree = json!([{
            "name": "run", "kind": 12,
            "range": {"start": {"line": 1, "character": 0}, "end": {"line": 3, "character": 1}},
            "selectionRange": {"start": {"line": 1, "character": 9}, "end": {"line": 1, "character": 12}},
            "children": [{
                "name": "inner", "kind": 12,
                "range": {"start": {"line": 2, "character": 2}, "end": {"line": 2, "character": 30}}
            }]
        }]);
        let mut flat = Vec::new();
        flatten_symbols(&tree, &mut flat);
        assert_eq!(
            enclosing_callable(&flat, 2, 10).map(|s| s["name"].clone()),
            Some(json!("inner"))
        );
        assert_eq!(
            enclosing_callable(&flat, 3, 0).map(|s| s["name"].clone()),
            Some(json!("run"))
        );
        assert!(enclosing_callable(&flat, 9, 0).is_none());
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
        assert_eq!(failure.code, "cancelled");
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
        assert_eq!(failure.code, "cancelled");
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
                &crate::tools::cancel::NeverCancel,
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
        assert_eq!(failure.code, "cancelled");
        let scope = Scope::new(root_str, vec!["*.ts".into()]);
        let complete = candidate_files(
            &scope,
            "target",
            &HashSet::new(),
            &policy(root.path()),
            &crate::tools::cancel::NeverCancel,
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

    #[test]
    fn applies_only_to_ts_js_incoming_operations() {
        assert!(applies(Some("javascript"), "references"));
        assert!(applies(Some("typescript"), "callers"));
        assert!(!applies(Some("rust"), "references"));
        assert!(!applies(Some("typescript"), "definition"));
        assert!(!applies(None, "references"));
    }
}
