//! Request anchor resolution: the zero-based LSP position the server is
//! asked about, resolved on exactly the text synchronized with `didOpen`,
//! plus the one-based `resolvedSymbol` receipt presented to the caller.

use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::resolver::resolve_position_in_file_content;
use octocode_engine::lsp::types::{LspFuzzyPosition, LspResolvedSymbol};
use octocode_io::memo::{Memo, MemoLimits};
use serde_json::{Value, json};

/// Symbol anchors resolved per `(path, name, line, occurrence, source
/// text)`: resolution is a pure function of them (a parse past its budget
/// is an error, never stored), and a warm repeat skips the parse. The 256
/// most recently used are kept.
static RESOLVED: Memo<[u8; 32], LspResolvedSymbol> = Memo::new(MemoLimits {
    max_entries: 256,
    max_weight: 256,
    keep_newest: false,
});

/// A resolved anchor: zero-based LSP `line`/`character` for the server, and
/// the public receipt (one-based) when the operation is anchored.
#[derive(Debug, PartialEq)]
pub(super) struct Anchor {
    pub(super) line: u32,
    pub(super) character: u32,
    pub(super) resolved_symbol: Option<Value>,
}

/// A zero-based LSP position in `path`: where a request, or one of its
/// recovery steps, asks the server.
#[derive(Clone, Copy, Debug)]
pub(super) struct Position<'a> {
    pub(super) path: &'a str,
    pub(super) line: u32,
    pub(super) character: u32,
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
    if !is_anchored(lsp_search_query::operation(query)) {
        return Ok(Anchor {
            line: 0,
            character: 0,
            resolved_symbol: None,
        });
    }
    if let Some(name) = lsp_search_query::symbol_name(query) {
        let source = source.ok_or_else(|| {
            "symbolName anchors require a readable source file in path".to_owned()
        })?;
        let resolved = resolve_symbol(
            path,
            source,
            name,
            lsp_search_query::anchor_line(query),
            lsp_search_query::occurrence(query),
        )?;
        // The receipt adds what the request did not say: the column, and the
        // line only when the symbol sat off its `line`. The name is the
        // request's own `symbolName`.
        let mut symbol = json!({
            "path": super::render::uri_to_path(canonical_uri),
            "foundAtCharacter": resolved.position.character + 1
        });
        if let Some(occurrence) = lsp_search_query::occurrence(query) {
            symbol["occurrence"] = json!(occurrence);
        }
        if resolved.line_offset != 0
            || lsp_search_query::anchor_line(query) != Some(resolved.found_at_line)
        {
            symbol["foundAtLine"] = json!(resolved.found_at_line);
        }
        if resolved.line_offset != 0 {
            symbol["lineDeviation"] = json!(resolved.line_offset.unsigned_abs());
        }
        if last_member_start(name).is_none() {
            disclose_same_line(&mut symbol, &resolved, name);
        }
        return Ok(Anchor {
            line: resolved.position.line,
            character: resolved.position.character,
            resolved_symbol: Some(symbol),
        });
    }
    Err("lspSearch requires symbolName and line".to_owned())
}

/// A line that spells the name more than once was resolved to one
/// occurrence (`occurrence`, default the first): the receipt counts them and
/// names each other one by the `occurrence` that selects it, its column, and
/// the qualified spelling (`target.config`) that anchors on it without a
/// hint.
fn disclose_same_line(symbol: &mut Value, resolved: &LspResolvedSymbol, name: &str) {
    let columns = &resolved.occurrences_on_line;
    if columns.len() < 2 {
        return;
    }
    let alternatives = columns
        .iter()
        .enumerate()
        .filter(|(_, column)| **column != resolved.position.character)
        .map(|(order, column)| {
            let mut alternative = json!({"occurrence": order, "foundAtCharacter": column + 1});
            if let Some(qualified) = qualified_at(&resolved.line_content, *column, name) {
                alternative["symbolName"] = json!(qualified);
            }
            alternative
        })
        .collect::<Vec<_>>();
    symbol["occurrencesOnLine"] = json!(columns.len());
    symbol["alternatives"] = json!(alternatives);
}

/// `qualifier.name` (or `::`, `->`) when the occurrence at UTF-16 `column`
/// of `line` is a member access; `None` for a bare name.
fn qualified_at(line: &str, column: u32, name: &str) -> Option<String> {
    let at = line
        .char_indices()
        .scan(0_u32, |units, (byte, ch)| {
            let start = *units;
            *units += crate::num::u32_of_usize(ch.len_utf16());
            Some((byte, start))
        })
        .find(|(_, start)| *start == column)
        .map(|(byte, _)| byte)?;
    let before = &line[..at];
    let (separator, rest) = ["::", "->", "."].iter().find_map(|separator| {
        before
            .strip_suffix(separator)
            .map(|rest| (*separator, rest))
    })?;
    let start = rest
        .char_indices()
        .rev()
        .take_while(|(_, ch)| octocode_engine::text::is_ident_char(*ch))
        .last()
        .map(|(byte, _)| byte)?;
    Some(format!("{}{separator}{name}", &rest[start..]))
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
    occurrence: Option<u32>,
) -> Result<LspResolvedSymbol, String> {
    let key: [u8; 32] = {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        // The source is hashed in place, never copied into the key.
        for part in [
            path,
            name,
            &format!("{line_hint:?}\u{0}{occurrence:?}"),
            source,
        ] {
            digest.update((part.len() as u64).to_le_bytes());
            digest.update(part.as_bytes());
        }
        digest.finalize().into()
    };
    if let Some(hit) = RESOLVED.get(&key) {
        return Ok(hit.value.as_ref().clone());
    }
    let resolved = resolve_symbol_in(path, source, name, line_hint, occurrence)?;
    RESOLVED.insert(key, resolved.clone(), 1, None);
    Ok(resolved)
}

fn resolve_symbol_in(
    path: &str,
    source: &str,
    name: &str,
    line_hint: Option<u32>,
    occurrence: Option<u32>,
) -> Result<LspResolvedSymbol, String> {
    let resolve = |symbol_name: &str| {
        resolve_position_in_file_content(
            path,
            source,
            &LspFuzzyPosition {
                symbol_name: symbol_name.to_owned(),
                line_hint,
                order_hint: occurrence,
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
    let is_identifier =
        !identifier.is_empty() && identifier.chars().all(octocode_engine::text::is_ident_char);
    let qualifier = name[..start].trim_end_matches(['.', ':', '-', '>']);
    (is_identifier && !qualifier.is_empty()).then_some(start)
}

/// UTF-16 width of the qualifier when the resolved line spells `name` at the
/// resolved column; `None` when the hit is not the qualified expression.
fn qualifier_width(resolved: &LspResolvedSymbol, name: &str, member_start: usize) -> Option<u32> {
    let column = resolved.position.character as usize;
    let line: Vec<u16> = resolved.line_content.encode_utf16().collect();
    let spelled: Vec<u16> = name.encode_utf16().collect();
    line.get(column..)?
        .starts_with(&spelled)
        .then(|| crate::num::u32_of_usize(name[..member_start].encode_utf16().count()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `config: target.config.clone()` spells `config` twice: the receipt
    /// counts them and names the other by occurrence, column and qualified
    /// name; a line with one occurrence says nothing more.
    #[test]
    fn a_repeated_name_on_the_line_names_its_alternatives() {
        let source = "fn f(target: T) -> L {\n    L { config: target.config.clone() }\n}\n";
        let query: LspSearchQuery = serde_json::from_value(json!({
            "operation": "definition", "path": "/w/a.rs", "symbolName": "config", "line": 2
        }))
        .expect("query");
        let anchor =
            resolve_anchor(&query, "/w/a.rs", "file:///w/a.rs", Some(source)).expect("anchor");
        let symbol = anchor.resolved_symbol.expect("receipt");
        assert_eq!(symbol["occurrencesOnLine"], 2, "{symbol}");
        assert_eq!(
            symbol["alternatives"],
            json!([{"occurrence": 1, "foundAtCharacter": 24, "symbolName": "target.config"}]),
            "{symbol}"
        );
        let single: LspSearchQuery = serde_json::from_value(json!({
            "operation": "definition", "path": "/w/a.rs", "symbolName": "target", "line": 1
        }))
        .expect("query");
        let anchor =
            resolve_anchor(&single, "/w/a.rs", "file:///w/a.rs", Some(source)).expect("anchor");
        assert!(
            anchor
                .resolved_symbol
                .expect("receipt")
                .get("alternatives")
                .is_none()
        );
    }

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
