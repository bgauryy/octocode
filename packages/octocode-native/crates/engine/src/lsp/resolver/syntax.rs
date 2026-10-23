//! The grammar-aware resolution: symbol candidates from a tree-sitter
//! walk, ranked by declaration, exactness and literal-ness.

use super::line_scan::{find_symbol_in_line, hit_for};
use super::{DEFAULT_RADIUS, check_budget, order_hint_error};
use crate::error::Result;
use crate::lsp::types::{LspFuzzyPosition, LspResolvedSymbol};
use crate::structural::kinds::is_comment_kind;
use crate::text::LineIndex;
use crate::text::utf8_offsets::{byte_to_utf16_offset, hide_bom_in_line};
use std::time::Instant;
use tree_sitter::Node;

#[derive(Clone)]
pub(super) struct SymbolCandidate {
    pub(super) line_index: usize,
    character: usize,
    pub(super) is_exact: bool,
    is_declaration: bool,
    /// The match sits inside a string literal (or similar textual node) rather
    /// than an identifier; ranked after identifiers on the same line.
    pub(super) is_literal: bool,
}

/// One step of the walk's root-to-current path: a node's kind and the field
/// it fills in its parent. Kept alongside the cursor so declaration checks
/// never call `Node::parent()` (which re-descends from the root each time).
#[derive(Clone, Copy)]
struct PathStep<'tree> {
    kind: &'tree str,
    field: Option<&'tree str>,
}

pub(super) fn collect_symbol_candidates(
    node: Node<'_>,
    content: &str,
    index: &LineIndex,
    symbol_name: &str,
    candidates: &mut Vec<SymbolCandidate>,
    deadline: Instant,
) -> Result<()> {
    let mut cursor = node.walk();
    let mut path = vec![PathStep {
        kind: node.kind(),
        field: None,
    }];
    loop {
        check_budget(deadline)?;
        let current = cursor.node();
        if current.is_named() && !is_ignored_node(current.kind()) {
            if let Some(mut candidate) = candidate_from_node(current, content, index, symbol_name) {
                candidate.is_declaration = candidate.is_exact && is_declaration_name(&path);
                candidates.push(candidate);
            }
            if cursor.goto_first_child() {
                path.push(PathStep {
                    kind: cursor.node().kind(),
                    field: cursor.field_name(),
                });
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                if let Some(step) = path.last_mut() {
                    *step = PathStep {
                        kind: cursor.node().kind(),
                        field: cursor.field_name(),
                    };
                }
                break;
            }
            if !cursor.goto_parent() {
                return Ok(());
            }
            path.pop();
        }
    }
}

fn candidate_from_node(
    node: Node<'_>,
    content: &str,
    index: &LineIndex,
    symbol_name: &str,
) -> Option<SymbolCandidate> {
    let text = node.utf8_text(content.as_bytes()).ok()?;
    let is_exact = exact_symbol_text(text, symbol_name);
    if !is_exact && node.named_child_count() > 0 && !is_symbolish_node(node.kind()) {
        return None;
    }

    let match_offset = if is_exact {
        text.find(symbol_name).unwrap_or(0)
    } else {
        find_symbol_in_line(text, symbol_name, 0)?
    };
    // Tree-sitter rows count only `\n`; derive the line from the byte offset
    // so a lone `\r` breaks lines here exactly as it does for the server.
    let (line_index, column) = index.position_of(node.start_byte() + match_offset);
    Some(SymbolCandidate {
        line_index,
        character: column,
        is_exact,
        // Set by the walk, which holds the ancestor path.
        is_declaration: false,
        is_literal: is_literal_node(node.kind()),
    })
}

pub(super) fn pick_candidate(
    mut candidates: Vec<SymbolCandidate>,
    fuzzy: &LspFuzzyPosition,
    lines: &[&str],
) -> Result<Option<LspResolvedSymbol>> {
    // Nested nodes (e.g. `string` and its `string_fragment` child) can report
    // the same position; keep one candidate per position, preferring the most
    // symbol-like reading so an orderHint counts each occurrence once.
    candidates.sort_by_key(|candidate| {
        (
            candidate.line_index,
            candidate.character,
            candidate.is_literal,
            !candidate.is_exact,
            !candidate.is_declaration,
        )
    });
    candidates.dedup_by_key(|candidate| (candidate.line_index, candidate.character));
    let order_hint = fuzzy.order_hint.unwrap_or(0) as usize;
    // Identifier positions (sorted by line, then column: `orderHint` order),
    // so the hit can say when its line spells the name more than once.
    let identifiers = candidates
        .iter()
        .filter(|candidate| !candidate.is_literal)
        .map(|candidate| (candidate.line_index, candidate.character))
        .collect::<Vec<_>>();

    let selected = match fuzzy.line_hint {
        Some(line_hint) if line_hint > 0 => {
            let target = line_hint as i32 - 1;
            let mut same_line: Vec<&SymbolCandidate> = candidates
                .iter()
                .filter(|candidate| candidate.line_index as i32 == target)
                .collect();
            // Identifier occurrences come first (in column order), then
            // mentions inside string literals, so orderHint indexes real
            // symbol uses before incidental text matches.
            same_line.sort_by_key(|candidate| (candidate.is_literal, candidate.character));
            // An explicit orderHint past this line's occurrences names none
            // of them: say so, never answer for another occurrence.
            if order_hint > 0 && !same_line.is_empty() && order_hint >= same_line.len() {
                return Err(order_hint_error(
                    &fuzzy.symbol_name,
                    line_hint,
                    same_line.len(),
                ));
            }
            if let Some(candidate) = same_line.get(order_hint) {
                Some((*candidate).clone())
            } else {
                candidates
                    .into_iter()
                    .filter(|candidate| {
                        (candidate.line_index as i32 - target).abs() <= DEFAULT_RADIUS
                    })
                    .min_by_key(|candidate| {
                        (
                            (candidate.line_index as i32 - target).abs(),
                            candidate.is_literal,
                            !candidate.is_exact,
                            !candidate.is_declaration,
                            candidate.line_index,
                            candidate.character,
                        )
                    })
            }
        }
        _ => candidates.into_iter().min_by_key(|candidate| {
            (
                !candidate.is_declaration,
                candidate.is_literal,
                !candidate.is_exact,
                candidate.line_index,
                candidate.character,
            )
        }),
    };
    let Some(selected) = selected else {
        return Ok(None);
    };

    let line = lines.get(selected.line_index).copied().unwrap_or_default();
    let mut hit = hit_for(
        line,
        selected.line_index,
        selected.character,
        fuzzy
            .line_hint
            .filter(|line_hint| *line_hint > 0)
            .map(|line_hint| selected.line_index as i32 - (line_hint as i32 - 1))
            .unwrap_or(0),
    );
    hit.occurrences_on_line = identifiers
        .into_iter()
        .filter(|(line_index, _)| *line_index == selected.line_index)
        .map(|(_, character)| {
            let (line, character) = hide_bom_in_line(selected.line_index, line, character);
            byte_to_utf16_offset(line, character) as u32
        })
        .collect();
    Ok(Some(hit))
}

fn is_ignored_node(kind: &str) -> bool {
    is_comment_kind(kind) || kind == "ERROR"
}

fn is_literal_node(kind: &str) -> bool {
    // `template_string` is covered by "string"; C++ `template_*` kinds are
    // identifier-like and deliberately not treated as literals.
    kind.contains("string") || is_comment_kind(kind)
}

fn is_symbolish_node(kind: &str) -> bool {
    kind.contains("identifier")
        || kind.contains("name")
        || kind.contains("selector")
        || kind.contains("string")
        || kind.contains("key")
        || kind == "pair"
        || kind == "property"
        || kind == "attribute"
}

/// Most unfielded wrapper nodes between a name token and the declaration it
/// names (e.g. Go `a, b :=` puts the names in an `expression_list`).
const MAX_NAME_WRAPPER_HOPS: usize = 3;

/// `true` when the last node of `path` is the *name* of a declaration: it (or
/// a bounded chain of unfielded wrappers around it) fills a naming field
/// (`name`, C's `declarator`, `key`, …) of a declaring node. An identifier
/// merely used inside a class body, function body or object value is not.
fn is_declaration_name(path: &[PathStep<'_>]) -> bool {
    let mut child = path.len().saturating_sub(1);
    for _ in 0..=MAX_NAME_WRAPPER_HOPS {
        if child == 0 {
            return false;
        }
        let parent = path[child - 1].kind;
        match path[child].field {
            Some(field) => return is_naming_field(field) && is_declaring_kind(parent),
            None => child -= 1,
        }
    }
    false
}

fn is_naming_field(field: &str) -> bool {
    matches!(
        field,
        "name" | "declarator" | "key" | "left" | "pattern" | "label"
    )
}

/// Node kinds that introduce a name. Uses (`call`, `invocation`, member
/// `expression`s, `scoped`/`qualified` paths, imports) also carry `name`
/// fields in some grammars and are excluded.
fn is_declaring_kind(kind: &str) -> bool {
    const USE_MARKERS: [&str; 8] = [
        "call",
        "invocation",
        "expression",
        "reference",
        "scoped",
        "qualified",
        "import",
        "argument",
    ];
    const DECLARING_MARKERS: [&str; 20] = [
        "declaration",
        "definition",
        "declarator",
        "signature",
        "_item",
        "_spec",
        "class",
        "struct",
        "interface",
        "enum",
        "trait",
        "type_alias",
        "method",
        "function",
        "variant",
        "module",
        "namespace",
        "parameter",
        "pair",
        "assignment",
    ];
    !USE_MARKERS.iter().any(|marker| kind.contains(marker))
        && DECLARING_MARKERS.iter().any(|marker| kind.contains(marker))
}

fn exact_symbol_text(text: &str, symbol_name: &str) -> bool {
    if text == symbol_name {
        return true;
    }
    text.trim_matches(['"', '\'', '`'])
        .trim_start_matches(['.', '#', '$', '@'])
        == symbol_name
}
