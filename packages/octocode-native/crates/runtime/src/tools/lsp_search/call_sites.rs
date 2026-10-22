//! Call-hierarchy edges derived from references: whether a reference is a
//! call, the callable around it in a `documentSymbol` answer, and the caller
//! edges of a whole importer set. Any server's walk uses them when its own
//! call hierarchy cannot cross a binding.

use super::failure::{LspFailure, RequestFailures};
use super::importers::Importers;
use super::recovery::get_locations;
use super::scope::canonical;
use super::source::SourceCache;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;

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
/// before any request. A request the server fails is recorded in
/// `failures`.
pub(super) async fn callers_from_references(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    cancel: &dyn CancellationCheck,
    importers: &Importers,
    answered: &HashSet<String>,
    failures: &mut RequestFailures,
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
                failures.record(failure)?;
                continue;
            }
        };
        let Some(source) = sources.get(&anchor.path).await else {
            continue;
        };
        let calls = references
            .iter()
            .filter(|snippet| super::scope::canonical_uri(&snippet.uri) == file)
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
        // Without the outline each call is attributed to the file itself:
        // the call sites stay listed, and the failure is disclosed.
        let symbols = match client.get_document_symbols(anchor.path.clone()).await {
            Ok(symbols) => symbols,
            Err(error) => {
                failures.record_engine(&error)?;
                Default::default()
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
