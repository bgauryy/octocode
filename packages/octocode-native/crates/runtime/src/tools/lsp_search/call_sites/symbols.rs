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
}

impl FlatSymbol<'_> {
    /// Whether `range` is exactly this symbol's `selectionRange`.
    fn is_named_by(&self, range: &LspRange) -> bool {
        let point = |at: &LspExactPosition| (u64::from(at.line), u64::from(at.character));
        self.selection_start == point(&range.start) && self.selection_end == point(&range.end)
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
            let selection = symbol
                .get("selectionRange")
                .filter(|selection| !selection.is_null())
                .unwrap_or(range);
            out.push(FlatSymbol {
                name: symbol.get("name").unwrap_or(&Value::Null),
                kind: symbol.get("kind").unwrap_or(&Value::Null),
                range,
                selection,
                start: point(range, "start"),
                end: point(range, "end"),
                selection_start: point(selection, "start"),
                selection_end: point(selection, "end"),
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enclosing_callables_of_an_outline() {
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

    fn range(line: u32, start: u32, end: u32) -> LspRange {
        serde_json::from_value(json!({
            "start": {"line": line, "character": start},
            "end": {"line": line, "character": end}
        }))
        .expect("range")
    }

    /// A flat `SymbolInformation` list has no `selectionRange`: the selection
    /// is the whole `location.range`, which names only a symbol whose server
    /// gave just the name there.
    #[test]
    fn a_flat_symbol_information_list_selects_its_location_range() {
        let outline = json!([{
            "name": "read", "kind": 6,
            "location": {"uri": "file:///a.ts", "range": {
                "start": {"line": 1, "character": 2}, "end": {"line": 3, "character": 3}
            }}
        }]);
        let mut flat = Vec::new();
        flatten_symbols(&outline, &mut flat);
        assert_eq!(flat[0]["selectionRange"], flat[0]["range"]);
        assert!(!is_declaration_name(&flat, &range(1, 2, 6)));
        let named = json!([{
            "name": "read", "kind": 6,
            "location": {"uri": "file:///a.ts", "range": {
                "start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 6}
            }}
        }]);
        let mut flat = Vec::new();
        flatten_symbols(&named, &mut flat);
        assert!(is_declaration_name(&flat, &range(1, 2, 6)));
        assert!(!is_declaration_name(&flat, &range(2, 21, 25)));
    }
}
