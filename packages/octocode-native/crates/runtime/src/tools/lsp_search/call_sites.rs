//! Call-hierarchy edges derived from references: whether a reference is a
//! call, the callable around it, and the caller edges of a whole importer
//! set. Any server's walk uses them when its own call hierarchy cannot cross
//! a binding.
//!
//! A JS/TS importer is parsed once (oxc): its sites are calls by
//! TypeScript's own call-hierarchy rule, and their callers come from the
//! same parse. A file oxc cannot parse, or a site oxc does not decide, falls
//! back to the text rule (`(` follows the name, and the site is not a
//! declaration's own name in the server's `documentSymbol` outline), and the
//! outline gives the callers.

use super::failure::{LspFailure, RequestFailures};
use super::importers::{Anchor, CONCURRENT_FILE_VERIFICATIONS, Importers, ordered_concurrently};
use super::recovery::get_locations;
use super::scope::canonical;
use super::source::{Source, SourceCache};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use octocode_engine::graph::{GraphPosition, GraphRange};
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use octocode_engine::lsp::types::LspCodeSnippet;
use octocode_engine::signatures::{EnclosingCallable, ReferenceSite};
use octocode_engine::symbol_kind;
use octocode_engine::text::LineIndex;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

mod symbols;
pub(in crate::tools::lsp_search) mod text_rule;
pub(super) use symbols::{enclosing_callable, flatten_symbols};
use text_rule::is_text_call;

/// LSP `SymbolKind`s that own call sites: method, constructor, function.
const CALLABLE_KINDS: [u64; 3] = [
    symbol_kind::METHOD as u64,
    symbol_kind::CONSTRUCTOR as u64,
    symbol_kind::FUNCTION as u64,
];
/// LSP `symbol_kind::File`, for calls made at module top level.
const FILE_KIND: u64 = symbol_kind::FILE as u64;

/// Caller edges `(node, call sites)` derived from each verified importer's
/// references, for servers whose call hierarchy cannot cross the importer's
/// binding (CommonJS `module.exports = { f }` + destructured `require`).
/// A reference is a call by [`classify_sites`]; its caller is the innermost
/// function or method around it, or the file itself for a top-level call.
/// Files the walk's own call hierarchy already answered (`answered`,
/// canonical paths) are skipped before any request. A server that pipelines
/// requests is asked for several importers at once; the edges keep importer
/// order either way. A request the server fails is recorded in `failures`.
pub(super) async fn callers_from_references(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    importers: &Importers,
    answered: &HashSet<String>,
    failures: &mut RequestFailures,
) -> Result<Vec<(Value, Vec<Value>)>, LspFailure> {
    let files = importers
        .per_file()
        .filter(|anchor| !answered.contains(&canonical(&anchor.path)))
        .collect::<Vec<_>>();
    let answers = if client.serves_requests_concurrently() {
        let policy = sources.policy();
        ordered_concurrently(files, CONCURRENT_FILE_VERIFICATIONS, |anchor| async move {
            let mut own = SourceCache::new(policy);
            file_calls(client, &mut own, cancel, anchor).await
        })
        .await?
    } else {
        let mut answers = Vec::new();
        for anchor in files {
            answers.push(file_calls(client, sources, cancel, anchor).await?);
        }
        answers
    };
    let mut edges: Vec<(Value, Vec<Value>)> = Vec::new();
    let mut by_node: HashMap<(String, [u64; 4]), usize> = HashMap::new();
    for answer in answers {
        if let Some(failure) = answer.failure {
            failures.record(failure)?;
        }
        for (node, site) in answer.calls {
            let key = (
                node["uri"].as_str().unwrap_or_default().to_owned(),
                range_key(&node["selectionRange"]),
            );
            match by_node.get(&key) {
                Some(&index) => edges[index].1.push(site),
                None => {
                    by_node.insert(key, edges.len());
                    edges.push((node, vec![site]));
                }
            }
        }
    }
    Ok(edges)
}

/// The calls one importer file makes, `(caller node, site)` in reference
/// order, and the failure that cut them short.
#[derive(Default)]
struct FileCalls {
    calls: Vec<(Value, Value)>,
    failure: Option<LspFailure>,
}

/// [`FileCalls`] of one importer: its references in its own file, kept
/// when they are calls, each under its caller.
async fn file_calls(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    anchor: &Anchor,
) -> Result<FileCalls, LspFailure> {
    cancel.check().map_err(LspFailure::cancelled)?;
    let file = canonical(&anchor.path);
    // Only the importer's own sites are kept: read no other file's
    // snippet (a references answer names every file using the symbol).
    let own_file = own_file_policy(sources.policy(), &file);
    let references = match get_locations(
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
    {
        Ok(references) => references,
        Err(failure) => {
            return Ok(FileCalls {
                failure: Some(failure),
                ..FileCalls::default()
            });
        }
    };
    let Some(source) = sources.get(&anchor.path).await else {
        return Ok(FileCalls::default());
    };
    let own = references
        .iter()
        .filter(|snippet| super::scope::canonical_uri(&snippet.uri) == file)
        .collect::<Vec<_>>();
    if own.is_empty() {
        return Ok(FileCalls::default());
    }
    let starts = own
        .iter()
        .map(|snippet| GraphPosition {
            line: snippet.range.start.line,
            character: snippet.range.start.character,
        })
        .collect();
    let classified = classify_sites(cancel, &source, &anchor.path, starts).await?;
    // The text rule needs the outline (a declaration's own name is no call);
    // fetched once, only when the rule decides a site.
    let needs_text_rule = classified
        .as_ref()
        .is_none_or(|sites| sites.iter().any(|site| site.is_call.is_none()));
    let (symbols, failure) = if needs_text_rule {
        match client.get_document_symbols(anchor.path.clone()).await {
            Ok(symbols) => (symbols, None),
            Err(error) => (Arc::default(), Some(LspFailure::from_engine(&error))),
        }
    } else {
        (Arc::default(), None)
    };
    let mut flat = Vec::new();
    flatten_symbols(&symbols, &mut flat);
    let lines = LineIndex::new(&source.content);
    let text_call =
        |snippet: &LspCodeSnippet| is_text_call(&source.content, &lines, &flat, &snippet.range);
    let site = |snippet: &LspCodeSnippet| {
        json!({
            "start": {"line": snippet.range.start.line, "character": snippet.range.start.character},
            "end": {"line": snippet.range.end.line, "character": snippet.range.end.character}
        })
    };
    if let Some(sites) = classified {
        let calls: Vec<_> = own
            .into_iter()
            .zip(sites)
            .filter(|(snippet, classified)| {
                classified.is_call.unwrap_or_else(|| text_call(snippet))
            })
            .map(|(snippet, classified)| {
                let node = classified.caller.as_ref().map_or_else(
                    || file_node(&file, &snippet.uri),
                    |caller| callable_node(caller, &snippet.uri),
                );
                (node, site(snippet))
            })
            .collect();
        // Without the outline the text rule kept declaration names too.
        let failure = failure.filter(|_| !calls.is_empty());
        return Ok(FileCalls { calls, failure });
    }
    let calls = own
        .into_iter()
        .filter(|snippet| text_call(snippet))
        .collect::<Vec<_>>();
    if calls.is_empty() {
        return Ok(FileCalls::default());
    }
    // Without the outline each call is attributed to the file itself:
    // the call sites stay listed, and the failure is disclosed.
    let calls = calls
        .into_iter()
        .map(|call| {
            let node = enclosing_callable(&flat, call.range.start.line, call.range.start.character)
                .map_or_else(
                    || file_node(&file, &call.uri),
                    |symbol| {
                        json!({
                            "name": symbol["name"],
                            "kind": symbol["kind"],
                            "uri": call.uri,
                            "range": symbol["range"],
                            "selectionRange": symbol["selectionRange"],
                        })
                    },
                );
            (node, site(call))
        })
        .collect();
    Ok(FileCalls { calls, failure })
}

/// Whether each of `starts` in `source` (at `path`) is a call site, and the
/// callable around it, from one oxc parse on a blocking worker; `None` when
/// oxc cannot parse the file (or the worker failed): the caller then falls
/// back to the text rule.
pub(super) async fn classify_sites(
    cancel: &dyn CancellationCheck,
    source: &Arc<Source>,
    path: &str,
    starts: Vec<GraphPosition>,
) -> Result<Option<Vec<ReferenceSite>>, LspFailure> {
    let (source, path) = (Arc::clone(source), path.to_owned());
    Ok(super::blocking_cancellable(cancel, move |_| {
        octocode_engine::signatures::reference_sites(&source.content, &path, &starts)
    })
    .await?
    .flatten())
}

/// The caller node of an oxc-enclosed call site, LSP-shaped.
fn callable_node(caller: &EnclosingCallable, uri: &str) -> Value {
    json!({
        "name": caller.name,
        "kind": caller.kind,
        "uri": uri,
        "range": range_json(&caller.range),
        "selectionRange": range_json(&caller.selection_range),
    })
}

/// The caller node of a top-level call: the file itself.
fn file_node(file: &str, uri: &str) -> Value {
    let name = Path::new(file)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let origin = json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}});
    json!({"name": name, "kind": FILE_KIND, "uri": uri, "range": origin, "selectionRange": origin})
}

fn range_json(range: &GraphRange) -> Value {
    json!({
        "start": {"line": range.start.line, "character": range.start.character},
        "end": {"line": range.end.line, "character": range.end.character}
    })
}

/// An LSP range's four points, for keying nodes without comparing JSON.
fn range_key(range: &Value) -> [u64; 4] {
    let at = |edge: &str, field: &str| {
        range
            .get(edge)
            .and_then(|point| point.get(field))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    [
        at("start", "line"),
        at("start", "character"),
        at("end", "line"),
        at("end", "character"),
    ]
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
