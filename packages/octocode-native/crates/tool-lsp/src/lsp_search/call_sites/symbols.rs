//! A `documentSymbol` answer read as a flat list, and the callable around a
//! position in it: the outline of a file oxc cannot parse, and of the
//! server's own class callers.

use super::CALLABLE_KINDS;
use octocode_engine::lsp::types::{LspExactPosition, LspRange};
use serde_json::Value;

/// One symbol of a flattened `documentSymbol` answer, borrowed from it,
/// with its range points read once. Indexing by `name`, `kind`, `range`,
/// or `selectionRange` (defaults to `range`) gives the answer's field.
pub(in super::super) struct FlatSymbol<'a> {
    name: &'a Value,
    kind: &'a Value,
    range: &'a Value,
    selection: &'a Value,
    start: (u64, u64),
    end: (u64, u64),
    selection_start: (u64, u64),
    selection_end: (u64, u64),
    /// Whether the server sent a `selectionRange` (else it is `range`).
    selected: bool,
}

impl FlatSymbol<'_> {
    /// Whether `range` names this symbol: it starts where the symbol's
    /// `selectionRange` starts and ends inside it. Most servers select the
    /// name alone; sourcekit-lsp selects the signature (`f(_ x: Int)`), whose
    /// name a reference ranges alone. Without a sent selection (the whole
    /// `range` stands in) only that exact range names it.
    fn is_named_by(&self, range: &LspRange) -> bool {
        let point = |at: &LspExactPosition| (u64::from(at.line), u64::from(at.character));
        let end = point(&range.end);
        self.selection_start == point(&range.start)
            && if self.selected {
                end <= self.selection_end
            } else {
                end == self.selection_end
            }
    }
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
pub(in super::super) fn flatten_symbols<'a>(value: &'a Value, out: &mut Vec<FlatSymbol<'a>>) {
    for symbol in value.as_array().into_iter().flatten() {
        if let Some(range) = symbol
            .get("range")
            .or_else(|| symbol.pointer("/location/range"))
        {
            let sent = symbol
                .get("selectionRange")
                .filter(|selection| !selection.is_null());
            let selection = sent.unwrap_or(range);
            out.push(FlatSymbol {
                name: symbol.get("name").unwrap_or(&Value::Null),
                kind: symbol.get("kind").unwrap_or(&Value::Null),
                range,
                selection,
                start: point(range, "start"),
                end: point(range, "end"),
                selection_start: point(selection, "start"),
                selection_end: point(selection, "end"),
                selected: sent.is_some(),
            });
        }
        if let Some(children) = symbol.get("children") {
            flatten_symbols(children, out);
        }
    }
}

/// The `(line, character)` of a range's `edge` (`start` or `end`).
fn point(range: &Value, edge: &str) -> (u64, u64) {
    let at = |field: &str| {
        range
            .get(edge)
            .and_then(|point| point.get(field))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    (at("line"), at("character"))
}

/// Whether `range` is a symbol's own name (its `selectionRange`): a
/// declaration site, never a use. A flat `SymbolInformation` list has no
/// selection range, so the selection falls back to the symbol's whole
/// `location.range`: a reference matches only when the server gave just the
/// name there, and dropping it is then correct.
pub(in super::super) fn is_declaration_name(symbols: &[FlatSymbol<'_>], range: &LspRange) -> bool {
    symbols.iter().any(|symbol| symbol.is_named_by(range))
}

/// Innermost callable symbol whose range contains the position.
pub(in super::super) fn enclosing_callable<'s, 'a>(
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
mod tests;
