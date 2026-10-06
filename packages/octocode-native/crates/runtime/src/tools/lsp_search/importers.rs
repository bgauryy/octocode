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

use super::blocking_cancellable;
use super::failure::LspFailure;
use super::recovery::{get_locations, resolve_definition_chain, snippet_identity};
use super::render::{TS_LANGUAGE_IDS, uri_to_path, word_pattern};
use super::source::SourceCache;
use crate::policy::path::PathPolicy;
use crate::tools::cancel::CancellationCheck;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use octocode_engine::portable::search_ripgrep_cancellable;
use octocode_engine::types::RipgrepSearchOptions;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;

const TS_JS_GLOBS: [&str; 8] = [
    "*.ts", "*.tsx", "*.mts", "*.cts", "*.js", "*.jsx", "*.mjs", "*.cjs",
];
/// Candidate files opened and verified per request.
pub(super) const MAX_CANDIDATE_FILES: usize = 24;
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
    /// Lexical candidates beyond [`MAX_CANDIDATE_FILES`] were not checked.
    pub(super) capped: bool,
    /// A candidate could not be read, opened, or resolved, or the scan failed.
    pub(super) failed: bool,
}

/// True for operations whose answer lists places that point *at* the anchor.
pub(super) fn applies(language_id: Option<&str>, operation: &str) -> bool {
    language_id.is_some_and(|id| TS_LANGUAGE_IDS.contains(&id))
        && matches!(operation, "references" | "callers" | "callHierarchy")
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

fn canonical(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned())
}

/// TS/JS files under `workspace_root` that mention `symbol` as a word,
/// excluding `skip`, capped at [`MAX_CANDIDATE_FILES`]. The walk observes
/// `cancel` while it runs; `None` means the scan itself failed, which is
/// neither cancellation nor an empty candidate set.
async fn candidate_files(
    workspace_root: &str,
    symbol: &str,
    skip: &HashSet<String>,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<(Vec<String>, bool)>, LspFailure> {
    let options = RipgrepSearchOptions {
        path: workspace_root.to_owned(),
        pattern: word_pattern(symbol),
        files_only: Some(true),
        include: Some(TS_JS_GLOBS.iter().map(|glob| (*glob).to_owned()).collect()),
        ..RipgrepSearchOptions::default()
    };
    let filter = Arc::new(policy.clone());
    let root = workspace_root.to_owned();
    let Some(Ok(parsed)) = blocking_cancellable(cancel, move |stopped| {
        search_ripgrep_cancellable(options, filter, stopped)
    })
    .await?
    else {
        return Ok(None);
    };
    let files = parsed
        .files
        .into_iter()
        .map(|file| {
            let path = Path::new(&file.path);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                Path::new(&root).join(path)
            };
            canonical(&absolute.to_string_lossy())
        })
        .filter(|path| !skip.contains(path))
        .collect::<BTreeSet<_>>();
    let capped = files.len() > MAX_CANDIDATE_FILES;
    Ok(Some((
        files.into_iter().take(MAX_CANDIDATE_FILES).collect(),
        capped,
    )))
}

/// Ancestors checked above the server's workspace root for a JS monorepo root.
const MAX_SCAN_ROOT_ASCENT: usize = 6;

/// Where importers are searched: the server's workspace root, widened to the
/// nearest enclosing JS workspace root (`pnpm-workspace.yaml`, `lerna.json`,
/// or a `package.json` with `workspaces`) the read policy authorizes. A
/// package's own root (its `package.json`) hides sibling packages that import
/// it through a package-index re-export (`export { f } from "@scope/pkg"`).
/// The scan never leaves the anchor's repository: a monorepo above a nested
/// checkout (a directory with its own `.git`) is a different project, and its
/// ignore rules may hide the checkout entirely.
fn scan_root(workspace_root: &str, policy: &crate::policy::path::PathPolicy) -> String {
    let start = Path::new(workspace_root);
    let mut repository = Vec::new();
    for dir in start.ancestors().take(MAX_SCAN_ROOT_ASCENT + 1) {
        repository.push(dir);
        if dir.join(".git").exists() {
            break;
        }
    }
    repository
        .into_iter()
        .skip(1)
        .find(|dir| is_js_workspace_root(dir))
        .filter(|dir| {
            policy
                .validate(dir)
                .is_ok_and(|valid| valid.canonical.is_dir())
        })
        .map_or_else(
            || workspace_root.to_owned(),
            |dir| dir.to_string_lossy().into_owned(),
        )
}

fn is_js_workspace_root(dir: &Path) -> bool {
    dir.join("pnpm-workspace.yaml").is_file()
        || dir.join("lerna.json").is_file()
        || std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|manifest| manifest.get("workspaces").is_some())
}

/// The identifier covering a zero-based UTF-16 position, for anchors given
/// as `position` instead of `symbolName`.
pub(super) fn word_at(content: &str, line: u32, character: u32) -> Option<String> {
    let text = content.lines().nth(usize::try_from(line).ok()?)?;
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let mut units = 0u32;
    let mut at = None;
    for (index, c) in text.char_indices() {
        if units >= character {
            at = Some(index);
            break;
        }
        units += u32::try_from(c.len_utf16()).ok()?;
    }
    let at = at?;
    let start = text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(at, |(index, _)| index);
    let end = text[at..]
        .char_indices()
        .find(|(_, c)| !is_word(*c))
        .map_or(text.len(), |(index, _)| at + index);
    (start < end).then(|| text[start..end].to_owned())
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

    /// Record the scan on the row's coverage so readers and
    /// `inferred_project::annotate` know importers were checked.
    pub(super) fn annotate(&self, row: &mut Value) {
        if row.get("status").and_then(Value::as_str) == Some("error") {
            return;
        }
        let files = self.per_file().count();
        if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
            let coverage = payload
                .entry("coverage")
                .or_insert_with(|| json!({"scope":"languageServer","exhaustive":false}));
            coverage["importerScan"] = json!(if self.failed {
                SCAN_FAILED
            } else if self.capped {
                SCAN_CAPPED
            } else {
                SCAN_COMPLETE
            });
            coverage["verifiedImporterFiles"] = json!(files);
        }
    }
}

pub(super) const SCAN_COMPLETE: &str = "complete";
pub(super) const SCAN_CAPPED: &str = "capped";
pub(super) const SCAN_FAILED: &str = "failed";

/// Verified importer anchors for `symbol` declared at the request anchor.
/// `known_files` are files the server already reported; they are skipped.
#[allow(clippy::too_many_arguments)]
pub(super) async fn verified_anchors(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    symbol: &str,
    workspace_root: &str,
    anchor_path: &str,
    line: u32,
    character: u32,
    known_files: &HashSet<String>,
) -> Result<Importers, LspFailure> {
    if symbol.trim().is_empty() || workspace_root.is_empty() {
        return Ok(Importers::default());
    }
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
            failed: true,
            ..Importers::default()
        });
    };
    let mut skip = known_files.clone();
    skip.insert(canonical(anchor_path));
    let scan_root = scan_root(workspace_root, sources.policy());
    let Some((files, capped)) =
        candidate_files(&scan_root, symbol, &skip, sources.policy(), cancel).await?
    else {
        return Ok(Importers {
            failed: true,
            ..Importers::default()
        });
    };
    let (opened, open_failed) = open_candidates(client, sources, cancel, &files, symbol).await?;
    let (anchors, verify_failed) = verify_occurrences(
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
        capped,
        failed,
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
                    source.content.clone(),
                    Some(OPEN_SETTLE_MS),
                    Some(OPEN_READY_TIMEOUT_MS),
                )
                .await
                .map(|_| ())
        } else {
            client
                .open_document(file.clone(), source.content.clone())
                .await
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
/// Returns the verified anchors and whether any identity check failed.
async fn verify_occurrences(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    opened: Vec<OpenedFile>,
    declaration: &HashSet<String>,
) -> Result<(Vec<Anchor>, bool), LspFailure> {
    let mut failed = false;
    let mut anchors = Vec::new();
    for (file, spots) in opened {
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
    }
    Ok((anchors, failed))
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
    snippet_policy: &SnippetReadPolicy,
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
        let Ok(references) = get_locations(
            client,
            snippet_policy,
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
        let file = canonical(&anchor.path);
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
            .unwrap_or(Value::Null);
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
                        "selectionRange": symbol.get("selectionRange").unwrap_or(&symbol["range"]),
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

/// DocumentSymbol trees (and flat SymbolInformation lists) as one flat list
/// of `{name, kind, range, selectionRange}`.
fn flatten_symbols(value: &Value, out: &mut Vec<Value>) {
    for symbol in value.as_array().into_iter().flatten() {
        let range = symbol
            .get("range")
            .or_else(|| symbol.pointer("/location/range"))
            .cloned();
        if let Some(range) = range {
            out.push(json!({
                "name": symbol.get("name").cloned().unwrap_or(Value::Null),
                "kind": symbol.get("kind").cloned().unwrap_or(Value::Null),
                "range": range,
                "selectionRange": symbol.get("selectionRange").cloned().unwrap_or(Value::Null),
            }));
        }
        if let Some(children) = symbol.get("children") {
            flatten_symbols(children, out);
        }
    }
    for symbol in out.iter_mut() {
        if symbol["selectionRange"].is_null() {
            symbol["selectionRange"] = symbol["range"].clone();
        }
    }
}

/// Innermost callable symbol whose range contains the position.
fn enclosing_callable(symbols: &[Value], line: u32, character: u32) -> Option<&Value> {
    let point = |symbol: &Value, edge: &str| {
        (
            symbol
                .pointer(&format!("/range/{edge}/line"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
            symbol
                .pointer(&format!("/range/{edge}/character"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
    };
    let at = (u64::from(line), u64::from(character));
    symbols
        .iter()
        .filter(|symbol| {
            symbol["kind"]
                .as_u64()
                .is_some_and(|kind| CALLABLE_KINDS.contains(&kind))
        })
        .filter(|symbol| point(symbol, "start") <= at && at <= point(symbol, "end"))
        .max_by_key(|symbol| point(symbol, "start"))
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
        Err(failure) if failure.code == "lsp.cancelled" => Err(failure),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
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
        importers.annotate(&mut row);
        assert_eq!(row["payload"]["coverage"]["importerScan"], SCAN_FAILED);
    }

    #[test]
    fn word_at_finds_identifier_around_utf16_position() {
        let content = "const é = stageFile(a);\n";
        assert_eq!(word_at(content, 0, 10).as_deref(), Some("stageFile"));
        assert_eq!(word_at(content, 0, 14).as_deref(), Some("stageFile"));
        assert_eq!(word_at(content, 0, 8), None);
        assert_eq!(word_at(content, 3, 0), None);
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
            capped: false,
            failed: false,
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

    #[test]
    fn importer_scan_widens_to_the_enclosing_js_workspace_root() {
        let root =
            std::env::temp_dir().join(format!("octocode-lsp-scan-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let package = root.join("packages/element");
        std::fs::create_dir_all(package.join("src")).expect("package");
        let root = root.canonicalize().expect("canonical");
        let package = root.join("packages/element");
        std::fs::write(package.join("package.json"), r#"{"name":"@x/element"}"#).expect("pkg");
        let policy = |workspace: &Path| crate::tools::test_support::workspace_policy(workspace);
        let package_str = package.to_string_lossy().into_owned();
        // No monorepo marker: the package root stays the scan root.
        assert_eq!(scan_root(&package_str, &policy(&root)), package_str);
        std::fs::write(
            root.join("package.json"),
            r#"{"private":true,"workspaces":["packages/*"]}"#,
        )
        .expect("root manifest");
        assert_eq!(
            scan_root(&package_str, &policy(&root)),
            root.to_string_lossy()
        );
        // An unauthorized monorepo root never widens the scan.
        assert_eq!(scan_root(&package_str, &policy(&package)), package_str);
        // A repository boundary below the monorepo root stops the widening:
        // a nested checkout is its own project.
        std::fs::create_dir_all(root.join("packages/.git")).expect("nested repository");
        assert_eq!(scan_root(&package_str, &policy(&root)), package_str);
        std::fs::remove_dir_all(root.join("packages/.git")).expect("cleanup");
        std::fs::create_dir_all(package.join(".git")).expect("checkout");
        assert_eq!(scan_root(&package_str, &policy(&root)), package_str);
        std::fs::remove_dir_all(package.join(".git")).expect("cleanup");
        // The repository's own root still widens when it is the monorepo.
        std::fs::create_dir_all(root.join(".git")).expect("monorepo repository");
        assert_eq!(
            scan_root(&package_str, &policy(&root)),
            root.to_string_lossy()
        );
        let _ = std::fs::remove_dir_all(&root);
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
        assert_eq!(failure.code, "lsp.cancelled");
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
        assert_eq!(failure.code, "lsp.cancelled");
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
        let failure = candidate_files(
            &root_str,
            "target",
            &HashSet::new(),
            &policy(root.path()),
            &CancelAfterFirst(std::sync::atomic::AtomicUsize::new(0)),
        )
        .await
        .expect_err("cancellation is not an empty or complete scan");
        assert_eq!(failure.code, "lsp.cancelled");
        let complete = candidate_files(
            &root_str,
            "target",
            &HashSet::new(),
            &policy(root.path()),
            &crate::tools::cancel::NeverCancel,
        )
        .await
        .expect("scan")
        .expect("scan succeeded");
        assert_eq!(complete.0.len(), MAX_CANDIDATE_FILES);
        assert!(complete.1, "more candidates than the cap");
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
