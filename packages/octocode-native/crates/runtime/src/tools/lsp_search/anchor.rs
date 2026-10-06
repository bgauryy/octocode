//! Request anchor resolution: the zero-based LSP position the server is
//! asked about, resolved on exactly the text synchronized with `didOpen`,
//! plus the one-based `resolvedSymbol` receipt presented to the caller.

use super::LspSearchQuery;
use octocode_engine::lsp::resolver::{LineIndex, resolve_position_in_file_content};
use octocode_engine::lsp::types::{JsFuzzyPosition, JsResolvedSymbol};
use serde_json::{Value, json};

/// A resolved anchor: zero-based LSP `line`/`character` for the server, and
/// the public receipt (one-based) when the operation is anchored.
#[derive(Debug, PartialEq)]
pub(super) struct Anchor {
    pub(super) line: u32,
    pub(super) character: u32,
    pub(super) resolved_symbol: Option<Value>,
}

pub(super) fn is_anchored(operation: &str) -> bool {
    !matches!(
        operation,
        "documentSymbols" | "workspaceSymbol" | "diagnostic"
    )
}

/// Resolve the request anchor once. Document-wide operations need no anchor.
/// `source` is the document text sent in `didOpen` (`None` when no file was
/// synchronized); `symbolName` resolution and the explicit-`position` bounds
/// check both run on it, so the column is computed on the text the server
/// holds.
///
/// `resolvedSymbol` uses the public coordinate convention: `foundAtLine` and
/// `foundAtCharacter` are one-based (LSP line/UTF-16 character + 1).
pub(super) fn resolve_anchor(
    query: &LspSearchQuery,
    path: &str,
    canonical_uri: &str,
    source: Option<&str>,
) -> Result<Anchor, String> {
    if !is_anchored(&query.operation()) {
        return Ok(Anchor {
            line: 0,
            character: 0,
            resolved_symbol: None,
        });
    }
    if let Some(name) = query.symbol_name() {
        let source = source.ok_or_else(|| {
            "symbolName anchors require a readable source file in path".to_owned()
        })?;
        let resolved = resolve_symbol(path, source, name, query.line_hint(), query.order_hint())?;
        // The receipt adds what the request did not say: the column, and the
        // line only when the symbol sat off its `lineHint`. The name is the
        // request's own `symbolName`.
        let mut symbol = json!({
            "path": super::render::uri_to_path(canonical_uri),
            "foundAtCharacter": resolved.position.character + 1
        });
        if let Some(order_hint) = query.order_hint() {
            symbol["orderHint"] = json!(order_hint);
        }
        if resolved.line_offset != 0 || query.line_hint() != Some(resolved.found_at_line) {
            symbol["foundAtLine"] = json!(resolved.found_at_line);
        }
        if resolved.line_offset != 0 {
            symbol["lineDeviation"] = json!(resolved.line_offset.unsigned_abs());
        }
        return Ok(Anchor {
            line: resolved.position.line,
            character: resolved.position.character,
            resolved_symbol: Some(symbol),
        });
    }
    let (line, character) = query
        .position()
        .ok_or_else(|| "lspSearch requires position or symbolName+lineHint".to_owned())?;
    if let Some(error) = source.and_then(|source| position_bounds_error(source, (line, character)))
    {
        return Err(error);
    }
    Ok(Anchor {
        line,
        character,
        resolved_symbol: Some(json!({
            "path": super::render::uri_to_path(canonical_uri),
            "foundAtLine": line + 1,
            "foundAtCharacter": character + 1
        })),
    })
}

/// Resolve `name` near `line_hint`. A member-qualified name (`this.ns`,
/// `a.b.c`, `Type::method`, `node->next`) anchors on its last member: the
/// server answers for the identifier under the cursor, and the qualifier's
/// first token names a different symbol. The qualified spelling is located
/// first so a repeated member on the line resolves inside that expression;
/// when it is not spelled contiguously the bare member is resolved instead.
fn resolve_symbol(
    path: &str,
    source: &str,
    name: &str,
    line_hint: Option<u32>,
    order_hint: Option<u32>,
) -> Result<JsResolvedSymbol, String> {
    let resolve = |symbol_name: &str| {
        resolve_position_in_file_content(
            path,
            source,
            &JsFuzzyPosition {
                symbol_name: symbol_name.to_owned(),
                line_hint,
                order_hint,
            },
        )
        .map_err(|error| error.to_string())
    };
    let Some(member_start) = last_member_start(name) else {
        return resolve(name);
    };
    if let Ok(mut resolved) = resolve(name)
        && let Some(shift) = qualifier_width(&resolved, name, member_start)
    {
        resolved.position.character += shift;
        return Ok(resolved);
    }
    resolve(&name[member_start..])
}

/// Byte offset of the last member of a qualified name, when `name` has a
/// non-empty qualifier and ends in an identifier after `.`, `::`, or `->`.
fn last_member_start(name: &str) -> Option<usize> {
    let start = ["::", "->", "."]
        .iter()
        .filter_map(|separator| name.rfind(separator).map(|at| at + separator.len()))
        .max()?;
    let member = &name[start..];
    let identifier = member.strip_prefix('#').unwrap_or(member);
    let is_identifier = !identifier.is_empty()
        && identifier
            .chars()
            .all(|ch| ch == '_' || ch == '$' || ch.is_alphanumeric());
    let qualifier = name[..start].trim_end_matches(['.', ':', '-', '>']);
    (is_identifier && !qualifier.is_empty()).then_some(start)
}

/// UTF-16 width of the qualifier when the resolved line spells `name` at the
/// resolved column; `None` when the hit is not the qualified expression.
fn qualifier_width(resolved: &JsResolvedSymbol, name: &str, member_start: usize) -> Option<u32> {
    let column = resolved.position.character as usize;
    let line: Vec<u16> = resolved.line_content.encode_utf16().collect();
    let spelled: Vec<u16> = name.encode_utf16().collect();
    line.get(column..)?
        .starts_with(&spelled)
        .then(|| name[..member_start].encode_utf16().count() as u32)
}

/// An explicit zero-based `position` must name a line of the document and a
/// UTF-16 column within it (the end of the line is valid); servers otherwise
/// answer out-of-range positions with a silent `null`. Lines break on
/// `\r\n`, `\n`, and a lone `\r` — the LSP rule the server counts by.
/// `position` is the zero-based `(line, character)` anchor.
pub(super) fn position_bounds_error(source: &str, position: (u32, u32)) -> Option<String> {
    let (line, character) = position;
    let index = LineIndex::new(source);
    let line_count = index.len();
    let Some(text) = index.line(source, line as usize) else {
        return Some(format!(
            "line {} is past the end of the document: it has {line_count} lines (0-based lines 0-{}).",
            line,
            line_count - 1
        ));
    };
    let width = text.encode_utf16().count();
    (character as usize > width).then(|| {
        format!(
            "character {} is past the end of 0-based line {} ({width} UTF-16 units).",
            character, line
        )
    })
}
