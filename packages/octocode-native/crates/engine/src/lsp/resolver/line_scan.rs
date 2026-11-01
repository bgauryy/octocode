//! The lexical fallback: a symbol found by scanning lines, near a hinted
//! line or across the file, outside comments and string literals.

use super::{DEFAULT_RADIUS, check_budget, hinted_line, line_delta, order_hint_error};
use crate::error::{Error, Result};
use crate::lsp::types::{LspExactPosition, LspResolvedSymbol};
use crate::text::utf8_offsets::{byte_to_utf16_offset, hide_bom_in_line};
use std::time::Instant;

#[derive(Clone, Copy)]
struct QuoteState {
    in_single: bool,
    in_double: bool,
    in_template: bool,
    template_expr_depth: u32,
    escaped: bool,
}

impl QuoteState {
    fn new() -> Self {
        Self {
            in_single: false,
            in_double: false,
            in_template: false,
            template_expr_depth: 0,
            escaped: false,
        }
    }
}

pub(super) fn scan_near_line(
    lines: &[&str],
    symbol_name: &str,
    line_hint: u32,
    order_hint: usize,
) -> Result<LspResolvedSymbol> {
    let target = hinted_line(line_hint);
    let Some(&target_line) = lines.get(target).filter(|_| line_hint > 0) else {
        // The empty text after a final line break is no line of the file.
        let trailing = lines.len() > 1 && lines.last().is_some_and(|line| line.is_empty());
        return Err(Error::new(format!(
            "Line {line_hint} is out of range (file has {} lines)",
            lines.len() - usize::from(trailing)
        )));
    };

    if let Some(hit) = find_symbol_in_line(target_line, symbol_name, order_hint) {
        return Ok(hit_for(target_line, target, hit, 0));
    }
    if order_hint > 0 {
        let count = (0..)
            .take_while(|index| find_symbol_in_line(target_line, symbol_name, *index).is_some())
            .count();
        if count > 0 {
            return Err(order_hint_error(symbol_name, line_hint, count));
        }
    }

    for offset in 1..=DEFAULT_RADIUS {
        for line_index in [target.checked_sub(offset), target.checked_add(offset)]
            .into_iter()
            .flatten()
        {
            let Some(line) = lines.get(line_index) else {
                continue;
            };
            if let Some(hit) = find_symbol_in_line(line, symbol_name, 0) {
                return Ok(hit_for(
                    line,
                    line_index,
                    hit,
                    line_delta(target, line_index),
                ));
            }
        }
    }

    Err(Error::new(format!(
        "Could not find symbol '{symbol_name}' at or near line {line_hint}"
    )))
}

pub(super) fn scan_whole_file(
    lines: &[&str],
    symbol_name: &str,
    order_hint: usize,
    deadline: Instant,
) -> Result<Option<LspResolvedSymbol>> {
    let mut first_match = None;
    for (index, line) in lines.iter().enumerate() {
        check_budget(deadline)?;
        let hint = if first_match.is_none() { order_hint } else { 0 };
        let Some(character) = find_symbol_in_line(line, symbol_name, hint) else {
            continue;
        };
        let hit = hit_for(line, index, character, 0);
        if looks_like_declaration(line, symbol_name) {
            return Ok(Some(hit));
        }
        if first_match.is_none() {
            first_match = Some(hit);
        }
    }
    check_budget(deadline)?;
    Ok(first_match)
}

pub(super) fn hit_for(
    line: &str,
    line_index: usize,
    character: usize,
    line_offset: i32,
) -> LspResolvedSymbol {
    // A leading BOM is invisible to editors and LSP servers: row-0 columns and
    // line text exclude it, exactly as the AST surfaces report them.
    let (line, character) = hide_bom_in_line(line_index, line, character);
    LspResolvedSymbol {
        position: LspExactPosition {
            line: crate::num::saturating_u32(line_index),
            // `character` arrives as a BYTE offset within `line` (tree-sitter
            // columns and `str::find`/`match_indices` are byte-based). LSP
            // `character` is UTF-16 code units, so convert — otherwise any line
            // with non-ASCII before the symbol mis-positions the cursor.
            character: crate::num::saturating_u32(byte_to_utf16_offset(line, character)),
        },
        found_at_line: crate::num::saturating_u32(line_index).saturating_add(1),
        line_offset,
        line_content: line.to_owned(),
        occurrences_on_line: Vec::new(),
    }
}

fn looks_like_declaration(line: &str, symbol_name: &str) -> bool {
    const KEYWORDS: [&str; 14] = [
        "function",
        "class",
        "interface",
        "type",
        "enum",
        "const",
        "let",
        "var",
        "def",
        "struct",
        "fn",
        "trait",
        "func",
        "namespace",
    ];
    let trimmed = line.trim_start();
    KEYWORDS.iter().any(|keyword| {
        trimmed
            .strip_prefix(keyword)
            .map(|rest| contains_word(rest.trim_start_matches([' ', '*', '\t']), symbol_name))
            .unwrap_or(false)
    })
}

pub(super) fn find_symbol_in_line(
    line: &str,
    symbol_name: &str,
    order_hint: usize,
) -> Option<usize> {
    let code = strip_line_comment(line);
    let mut seen = 0usize;
    for (index, _) in code.match_indices(symbol_name) {
        if !crate::text::is_whole_word(code.as_bytes(), index, index + symbol_name.len()) {
            continue;
        }
        if seen == order_hint {
            return Some(index);
        }
        seen += 1;
    }
    None
}

fn contains_word(text: &str, word: &str) -> bool {
    crate::text::whole_word_starts(text, word).next().is_some()
}

fn strip_line_comment(line: &str) -> &str {
    let mut state = QuoteState::new();
    let mut iter = line.char_indices().peekable();
    while let Some((index, ch)) = iter.next() {
        if state.escaped {
            state.escaped = false;
            continue;
        }
        if ch == '\\' {
            state.escaped = true;
            continue;
        }
        if ch == '/'
            && iter.peek().map(|(_, next)| *next == '/').unwrap_or(false)
            && !state.in_single
            && !state.in_double
            && !state.in_template
        {
            return &line[..index];
        }
        if state.in_template
            && state.template_expr_depth == 0
            && ch == '$'
            && iter.peek().map(|(_, next)| *next == '{').unwrap_or(false)
        {
            state.template_expr_depth = 1;
            continue;
        }
        if state.template_expr_depth > 0 {
            if ch == '{' {
                state.template_expr_depth += 1;
            } else if ch == '}' {
                state.template_expr_depth -= 1;
            }
            continue;
        }
        if ch == '\'' && !state.in_double && !state.in_template {
            state.in_single = !state.in_single;
        } else if ch == '"' && !state.in_single && !state.in_template {
            state.in_double = !state.in_double;
        } else if ch == '`' && !state.in_single && !state.in_double {
            state.in_template = !state.in_template;
        }
    }
    line
}
