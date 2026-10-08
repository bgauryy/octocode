//! Request anchor resolution: the zero-based LSP position the server is
//! asked about, resolved on exactly the text synchronized with `didOpen`,
//! plus the one-based `resolvedSymbol` receipt presented to the caller.

use super::LspSearchQuery;
use octocode_engine::lsp::resolver::resolve_position_in_file_content;
use octocode_engine::lsp::types::{JsFuzzyPosition, JsResolvedSymbol};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Symbol anchors resolved per `(path, name, lineHint, orderHint, source
/// text)`: resolution is a pure function of them (a parse past its budget
/// is an error, never stored), and a warm repeat skips the parse.
static RESOLVED: LazyLock<Mutex<HashMap<String, JsResolvedSymbol>>> = LazyLock::new(Mutex::default);
/// Stored anchors per process; past it the store starts over.
const RESOLVED_MAX: usize = 256;

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
/// synchronized); `symbolName` resolution runs on it, so the column is
/// computed on the text the server holds.
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
    Err("lspSearch requires symbolName and lineHint".to_owned())
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
    let key = crate::digest::sha256(
        format!("{path}\u{0}{name}\u{0}{line_hint:?}\u{0}{order_hint:?}\u{0}{source}").as_bytes(),
    );
    if let Some(hit) = RESOLVED
        .lock()
        .ok()
        .and_then(|resolved| resolved.get(&key).cloned())
    {
        return Ok(hit);
    }
    let resolved = resolve_symbol_in(path, source, name, line_hint, order_hint)?;
    if let Ok(mut stored) = RESOLVED.lock() {
        if stored.len() >= RESOLVED_MAX {
            stored.clear();
        }
        stored.insert(key, resolved.clone());
    }
    Ok(resolved)
}

fn resolve_symbol_in(
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
    let name = impl_self_type(name).unwrap_or(name);
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

/// The implementing type of a Rust impl block's name: `Trait for Type`
/// (an astSearch impl row), `impl<…> Trait for Type<…>` or `impl Type`
/// (rust-analyzer's document symbols). Generic arguments are dropped so the
/// type's identifier is the anchor, the one an inherent impl row names.
/// `None` for any other name (an identifier never holds a space).
fn impl_self_type(name: &str) -> Option<&str> {
    let self_type = match name.rfind(" for ") {
        Some(at) => &name[at + " for ".len()..],
        None => {
            let rest = name.strip_prefix("impl")?;
            if !rest.starts_with([' ', '<']) {
                return None;
            }
            skip_generics(rest)
        }
    };
    let self_type = self_type.trim();
    let self_type = self_type.split('<').next().unwrap_or(self_type).trim();
    (!self_type.is_empty() && !self_type.contains(char::is_whitespace)).then_some(self_type)
}

/// `text` after a leading `<…>` generic list (balanced), trimmed.
fn skip_generics(text: &str) -> &str {
    let text = text.trim_start();
    if !text.starts_with('<') {
        return text;
    }
    let mut depth = 0_usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return text[at + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    text
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_anchor_never_answers_for_edited_text() {
        let path = "/memo/anchor.ts";
        let before = "export function moved() {}\n";
        let after = "\n\nexport function moved() {}\n";
        let first = resolve_symbol(path, before, "moved", Some(1), None).expect("first");
        let again = resolve_symbol(path, before, "moved", Some(1), None).expect("again");
        assert_eq!(
            (again.found_at_line, again.position.character),
            (first.found_at_line, first.position.character)
        );
        let edited = resolve_symbol(path, after, "moved", Some(1), None).expect("edited");
        assert_eq!(edited.found_at_line, 3);
        assert_ne!(edited.position.line, first.position.line);
    }

    /// A Rust impl row's name (astSearch `Trait for Type`, rust-analyzer's
    /// `impl Trait for Type` / `impl Type`) anchors on the implementing
    /// type, the identifier an inherent impl row names.
    #[test]
    fn impl_block_names_anchor_on_the_self_type() {
        let source =
            "struct Wrap<T>(T);\nimpl<T: Clone> std::fmt::Debug for Wrap<T> {}\nimpl Wrap<u8> {}\n";
        let type_column = source.lines().nth(1).unwrap().find("Wrap").unwrap() as u32;
        for name in [
            "std::fmt::Debug for Wrap",
            "impl<T: Clone> std::fmt::Debug for Wrap<T>",
            "Debug for Wrap",
        ] {
            let resolved = resolve_symbol_in("/a.rs", source, name, Some(2), None)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                (resolved.found_at_line, resolved.position.character),
                (2, type_column),
                "{name}"
            );
        }
        let inherent = resolve_symbol_in("/a.rs", source, "impl Wrap<u8>", Some(3), None)
            .expect("inherent impl");
        assert_eq!(
            (inherent.found_at_line, inherent.position.character),
            (3, 5)
        );
    }
}
