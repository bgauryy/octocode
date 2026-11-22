//! Request anchor resolution: the zero-based LSP position the server is
//! asked about, resolved on exactly the text synchronized with `didOpen`,
//! plus the one-based `resolvedSymbol` receipt presented to the caller.

use crate::lsp_search::lsp_search_query;
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
    /// Why the answer is anchored off the request's `line`, for the row's
    /// warnings.
    pub(super) warning: Option<String>,
    /// With a warning: the calls that find the line the name is spelled or
    /// declared on, for the row's `related`.
    pub(super) related: Vec<Value>,
}

impl Anchor {
    /// States on `row` why the answer is anchored off the request's line
    /// (`warning`), with the calls that find the line meant (`related`):
    /// evidence the answer may lack, so they are required leads.
    pub(super) fn disclose(row: &mut Value, warning: Option<String>, related: Vec<Value>) {
        octocode_runtime_base::response::paging::push_warnings(row, warning);
        for lead in related {
            octocode_runtime_base::response::result::attach_related(
                row,
                octocode_runtime_base::response::result::required_call(lead),
            );
        }
    }
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
            related: Vec::new(),
            warning: None,
        });
    }
    if let Some(name) = lsp_search_query::symbol_name(query) {
        let source = source.ok_or_else(|| {
            "symbolName anchors require a readable source file in path".to_owned()
        })?;
        if let Some(asked) = lsp_search_query::anchor_line(query) {
            past_the_end(source, name, asked)?;
        }
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
        let warning = lsp_search_query::anchor_line(query)
            .filter(|_| resolved.line_offset != 0)
            .map(|asked| moved_anchor(name, asked, source, &resolved, &symbol));
        let related = match (&warning, lsp_search_query::path(query)) {
            (Some(_), Some(asked)) => {
                super::failure::line_finders(name, &super::render::uri_to_path(asked))
            }
            _ => Vec::new(),
        };
        return Ok(Anchor {
            line: resolved.position.line,
            character: resolved.position.character,
            resolved_symbol: Some(symbol),
            warning,
            related,
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

/// Lines a refusal or a warning lists at most; the rest are counted.
const LISTED_LINES: usize = 8;

/// `lines` as text, the first [`LISTED_LINES`] named and the rest counted.
fn listed_lines(lines: &[u32]) -> String {
    let named = lines
        .iter()
        .take(LISTED_LINES)
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    match lines.len().saturating_sub(LISTED_LINES) {
        0 => named,
        more => format!("{named} and {more} more"),
    }
}

/// A one-based `asked` line past the end of `source` names no line of the
/// file: anchoring on it would choose an identity by name alone, so the read
/// is refused, stating the file's length and the lines that spell the name.
fn past_the_end(source: &str, name: &str, asked: u32) -> Result<(), String> {
    let length = source.lines().count();
    if usize::try_from(asked).is_ok_and(|asked| asked <= length) {
        return Ok(());
    }
    let mut lines = super::importers::occurrences(source, name)
        .into_iter()
        .map(|(line, _, _)| line + 1)
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines.dedup();
    let spelled = if lines.is_empty() {
        format!("`{name}` is not spelled in it")
    } else {
        format!("`{name}` is spelled on line(s) {}", listed_lines(&lines))
    };
    Err(format!(
        "Line {asked} is out of range (file has {}); {spelled}",
        octocode_engine::text::counted(length, "line", "lines")
    ))
}

/// The warning of an anchor that moved: the name is not on the one-based
/// `asked` line, so the answer is for the spelling on the line `resolved`
/// names. When other lines spell the name too (the receipt's
/// `alternatives`, in `symbol`), the answer may be for another identity
/// than the one meant, and the warning lists them, every spelling of the
/// name in the file, not only the receipt's alternatives.
fn moved_anchor(
    name: &str,
    asked: u32,
    source: &str,
    resolved: &LspResolvedSymbol,
    symbol: &Value,
) -> String {
    let mut others = symbol
        .get("alternatives")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|alternative| alternative.get("foundAtLine").and_then(Value::as_u64))
        .filter_map(|line| u32::try_from(line).ok())
        .collect::<Vec<_>>();
    // Spellings the receipt does not name (it lists the nearest rivals only)
    // may be other symbols of that name too: the warning lists every line.
    let mut unnamed = super::importers::occurrences(source, name)
        .into_iter()
        .map(|(line, _, _)| line + 1)
        .filter(|line| *line != resolved.found_at_line && !others.contains(line))
        .collect::<Vec<_>>();
    unnamed.sort_unstable();
    unnamed.dedup();
    others.extend(unnamed);
    let rivals = if others.is_empty() {
        String::new()
    } else {
        format!(
            "; it is also spelled on line(s) {}, so this answer may be for another identity: send the line of the one meant",
            listed_lines(&others)
        )
    };
    format!(
        "`{name}` is not on line {asked}: anchored on line {}{rivals}.",
        resolved.found_at_line
    )
}

/// A name that is not on its `line` resolved to the nearest line that
/// anchors it, which may hold another symbol of that spelling. Every other
/// line the resolver chose among (within its own reach of the one-based
/// `asked` line, [`DEFAULT_RADIUS`]) is a rival reading of a stale line
/// number, and so is the nearest line on the other side of `asked`, however
/// far: the resolver chose by nearness alone. A line out of that reach of
/// every spelling resolved to the file's one declaration, not by nearness:
/// the nearest line on its own side, when nearer than that declaration, is
/// a rival too. The receipt names each
/// as an alternative, nearest first, with the `line` that anchors on it and
/// its distance.
fn disclose_rival_lines(
    symbol: &mut Value,
    path: &str,
    source: &str,
    name: &str,
    asked: u32,
    resolved: &LspResolvedSymbol,
) {
    let below = resolved.found_at_line > asked;
    let by_nearness = resolved.found_at_line.abs_diff(asked) as usize <= DEFAULT_RADIUS;
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
    // The nearest anchoring line found on the other side, and on the
    // resolved line's own side: wanted only when nearness did not choose,
    // and only nearer to `asked` than the resolved line.
    let (mut crossed, mut beside) = (false, by_nearness);
    let resolved_distance = resolved.found_at_line.abs_diff(asked);
    let mut rivals = Vec::new();
    for line in lines {
        beside |= line.abs_diff(asked) >= resolved_distance;
        let chosen_among = line.abs_diff(asked) as usize <= DEFAULT_RADIUS;
        let crossing = !crossed && other_side(line);
        let nearest_beside = !beside && !other_side(line);
        if !chosen_among && crossed && beside {
            break;
        }
        if !chosen_among && !crossing && !nearest_beside {
            continue;
        }
        let Some(rival) = anchors(line) else {
            continue;
        };
        crossed |= crossing;
        beside |= nearest_beside;
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

/// Whether the request's anchor fails only by its `occurrence`: the name
/// stands on the requested line, which the same resolution without the
/// `occurrence` finds. The resolver's error then states the whole fix (how
/// many occurrences the line has), and nothing about the name or the line
/// is left to locate.
pub(super) fn misses_only_by_occurrence(
    query: &LspSearchQuery,
    path: &str,
    source: Option<&str>,
) -> bool {
    let (Some(name), Some(source), Some(line)) = (
        lsp_search_query::symbol_name(query),
        source,
        lsp_search_query::anchor_line(query),
    ) else {
        return false;
    };
    lsp_search_query::occurrence(query).is_some_and(|occurrence| occurrence > 0)
        && resolve_symbol(path, source, name, Some(line), None)
            .is_ok_and(|resolved| resolved.line_offset == 0 && resolved.found_at_line == line)
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
mod tests;
