//! Request anchor resolution: the zero-based LSP position the server is
//! asked about, resolved on exactly the text synchronized with `didOpen`,
//! plus the one-based `resolvedSymbol` receipt presented to the caller.

use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::resolver::{DEFAULT_RADIUS, resolve_position_in_file_content};
use octocode_engine::lsp::types::{LspFuzzyPosition, LspResolvedSymbol};
use serde_json::{Value, json};

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
        if let Some(asked) =
            lsp_search_query::anchor_line(query).filter(|_| resolved.line_offset != 0)
        {
            disclose_rival_lines(&mut symbol, path, source, name, asked, &resolved);
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
    // One index of the line serves every occurrence on it.
    let line = &resolved.line_content;
    let index = octocode_engine::text::LineIndex::tree_sitter(line);
    let alternatives = columns
        .iter()
        .enumerate()
        .filter(|(_, column)| **column != resolved.position.character)
        .map(|(order, column)| {
            let mut alternative = json!({"occurrence": order, "foundAtCharacter": column + 1});
            let byte = index.position_to_byte(line, 0, *column) as usize;
            if let Some(qualified) = qualified_at(line, byte, name) {
                alternative["symbolName"] = json!(qualified);
            }
            alternative
        })
        .collect::<Vec<_>>();
    symbol["occurrencesOnLine"] = json!(columns.len());
    symbol["alternatives"] = json!(alternatives);
}

/// A name that is not on its `line` resolved to the nearest line that
/// anchors it, which may hold another symbol of that spelling. Every other
/// line the resolver chose among (within its own reach of the one-based
/// `asked` line, [`DEFAULT_RADIUS`]) is a rival reading of a stale line
/// number, and so is the nearest line on the other side of `asked`, however
/// far: the resolver chose by nearness alone. The receipt names each as an
/// alternative, nearest first, with the `line` that anchors on it and its
/// distance.
fn disclose_rival_lines(
    symbol: &mut Value,
    path: &str,
    source: &str,
    name: &str,
    asked: u32,
    resolved: &LspResolvedSymbol,
) {
    let below = resolved.found_at_line > asked;
    let other_side = |line: u32| if below { line < asked } else { line > asked };
    let mut lines = super::importers::occurrences(source, name)
        .into_iter()
        .map(|(line, _, _)| line + 1)
        .filter(|line| *line != resolved.found_at_line)
        .collect::<Vec<_>>();
    lines.sort_by_key(|line| (line.abs_diff(asked), *line));
    lines.dedup();
    // The resolver judges each spelling as it judged the anchor: a line it
    // anchors on exactly is one a request can name.
    let anchors = |line: u32| {
        resolve_symbol(path, source, name, Some(line), None)
            .ok()
            .filter(|rival| rival.line_offset == 0)
    };
    let mut crossed = false;
    let mut rivals = Vec::new();
    for line in lines {
        let chosen_among = line.abs_diff(asked) as usize <= DEFAULT_RADIUS;
        let crossing = !crossed && other_side(line);
        if !chosen_among && crossed {
            break;
        }
        if !chosen_among && !crossing {
            continue;
        }
        let Some(rival) = anchors(line) else {
            continue;
        };
        crossed |= crossing;
        rivals.push(json!({
            "foundAtLine": rival.found_at_line,
            "foundAtCharacter": rival.position.character + 1,
            "lineDeviation": rival.found_at_line.abs_diff(asked),
        }));
    }
    if rivals.is_empty() {
        return;
    }
    match symbol.get_mut("alternatives").and_then(Value::as_array_mut) {
        Some(listed) => listed.extend(rivals),
        None => symbol["alternatives"] = json!(rivals),
    }
}

/// `qualifier.name` (or `::`, `->`) when the occurrence at byte `at` of
/// `line` is a member access; `None` for a bare name.
fn qualified_at(line: &str, at: usize, name: &str) -> Option<String> {
    let before = line.get(..at)?;
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
        .then(|| octocode_engine::num::saturating_u32(name[..member_start].encode_utf16().count()))
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

    /// A name that is not on its line resolves to the nearest line that
    /// spells it; the nearest one on the other side of the asked line is
    /// named as the alternative, however far, and a name found on its own
    /// line names none.
    #[test]
    fn a_name_off_its_line_names_the_nearest_line_on_the_other_side() {
        let source = format!(
            "function run() {{}}\n{}obj.run();\n",
            "// filler\n".repeat(8)
        );
        let receipt = |line: u32| {
            let query: LspSearchQuery = serde_json::from_value(json!({
                "operation": "definition", "path": "/w/a.ts", "symbolName": "run", "line": line
            }))
            .expect("query");
            resolve_anchor(&query, "/w/a.ts", "file:///w/a.ts", Some(&source))
                .expect("anchor")
                .resolved_symbol
                .expect("receipt")
        };
        // Line 7 is three lines from the call on line 10 and six from the
        // declaration on line 1.
        let off = receipt(7);
        assert_eq!(off["foundAtLine"], 10, "{off}");
        assert_eq!(off["lineDeviation"], 3, "{off}");
        assert_eq!(
            off["alternatives"],
            json!([{"foundAtLine": 1, "foundAtCharacter": 10, "lineDeviation": 6}]),
            "{off}"
        );
        let other = receipt(3);
        assert_eq!(other["foundAtLine"], 1, "{other}");
        assert_eq!(
            other["alternatives"],
            json!([{"foundAtLine": 10, "foundAtCharacter": 5, "lineDeviation": 7}]),
            "{other}"
        );
        assert!(receipt(10).get("alternatives").is_none());
        assert!(receipt(1).get("alternatives").is_none());
    }

    /// A stale line resolves to the nearest spelling the resolver accepts;
    /// every other spelling it chose among is as likely the one meant, on
    /// either side of the asked line, and the receipt names each (nearest
    /// first), with the nearest one on the other side however far.
    #[test]
    fn a_name_off_its_line_names_every_spelling_the_resolver_chose_among() {
        let source = format!(
            "fn a() {{}}\nfn check() {{}}\nfn b() {{ check(); }}\n// filler\n// asked\n{}fn c() {{ check(); }}\n",
            "// filler\n".repeat(14)
        );
        let query: LspSearchQuery = serde_json::from_value(json!({
            "operation": "hover", "path": "/w/a.rs", "symbolName": "check", "line": 5
        }))
        .expect("query");
        let receipt = resolve_anchor(&query, "/w/a.rs", "file:///w/a.rs", Some(&source))
            .expect("anchor")
            .resolved_symbol
            .expect("receipt");
        assert_eq!(receipt["foundAtLine"], 3, "{receipt}");
        assert_eq!(receipt["lineDeviation"], 2, "{receipt}");
        assert_eq!(
            receipt["alternatives"],
            json!([
                {"foundAtLine": 2, "foundAtCharacter": 4, "lineDeviation": 3},
                {"foundAtLine": 20, "foundAtCharacter": 10, "lineDeviation": 15},
            ]),
            "{receipt}"
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
            let resolved = resolve_symbol("/a.rs", source, name, Some(2), None)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                (resolved.found_at_line, resolved.position.character),
                (2, type_column),
                "{name}"
            );
        }
        let inherent =
            resolve_symbol("/a.rs", source, "impl Wrap<u8>", Some(3), None).expect("inherent impl");
        assert_eq!(
            (inherent.found_at_line, inherent.position.character),
            (3, 5)
        );
    }
}
