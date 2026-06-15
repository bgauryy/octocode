use crate::types::{JsExactPosition, JsFuzzyPosition, JsResolvedSymbol};
use napi::{Error, Result, Status};
use std::fs;

const DEFAULT_RADIUS: i32 = 5;

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

pub fn resolve_position(file_path: String, fuzzy: JsFuzzyPosition) -> Result<JsResolvedSymbol> {
    let content = fs::read_to_string(&file_path).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to read {file_path}: {err}"),
        )
    })?;
    resolve_position_from_content(content, fuzzy)
}

pub fn resolve_position_from_content(
    content: String,
    fuzzy: JsFuzzyPosition,
) -> Result<JsResolvedSymbol> {
    let lines: Vec<&str> = content
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let order_hint = fuzzy.order_hint.unwrap_or(0) as usize;

    match fuzzy.line_hint {
        None | Some(0) => {
            scan_whole_file(&lines, &fuzzy.symbol_name, order_hint).ok_or_else(|| {
                Error::new(
                    Status::GenericFailure,
                    format!(
                        "Could not find symbol '{}' anywhere in the file",
                        fuzzy.symbol_name
                    ),
                )
            })
        }
        Some(line_hint) => scan_near_line(&lines, &fuzzy.symbol_name, line_hint, order_hint),
    }
}

fn scan_near_line(
    lines: &[&str],
    symbol_name: &str,
    line_hint: u32,
    order_hint: usize,
) -> Result<JsResolvedSymbol> {
    let target = line_hint as i32 - 1;
    if target < 0 || target as usize >= lines.len() {
        return Err(Error::new(
            Status::InvalidArg,
            format!(
                "Line {line_hint} is out of range (file has {} lines)",
                lines.len()
            ),
        ));
    }

    if let Some(hit) = find_symbol_in_line(lines[target as usize], symbol_name, order_hint) {
        return Ok(hit_for(lines[target as usize], target as usize, hit, 0));
    }

    for offset in 1..=DEFAULT_RADIUS {
        for delta in [-offset, offset] {
            let line_index = target + delta;
            if line_index < 0 || line_index as usize >= lines.len() {
                continue;
            }
            if let Some(hit) = find_symbol_in_line(lines[line_index as usize], symbol_name, 0) {
                return Ok(hit_for(
                    lines[line_index as usize],
                    line_index as usize,
                    hit,
                    delta,
                ));
            }
        }
    }

    Err(Error::new(
        Status::GenericFailure,
        format!("Could not find symbol '{symbol_name}' at or near line {line_hint}"),
    ))
}

fn scan_whole_file(
    lines: &[&str],
    symbol_name: &str,
    order_hint: usize,
) -> Option<JsResolvedSymbol> {
    let mut first_match = None;
    for (index, line) in lines.iter().enumerate() {
        let hint = if first_match.is_none() { order_hint } else { 0 };
        let Some(character) = find_symbol_in_line(line, symbol_name, hint) else {
            continue;
        };
        let hit = hit_for(line, index, character, 0);
        if looks_like_declaration(line, symbol_name) {
            return Some(hit);
        }
        if first_match.is_none() {
            first_match = Some(hit);
        }
    }
    first_match
}

fn hit_for(line: &str, line_index: usize, character: usize, line_offset: i32) -> JsResolvedSymbol {
    JsResolvedSymbol {
        position: JsExactPosition {
            line: line_index as u32,
            character: character as u32,
        },
        found_at_line: line_index as u32 + 1,
        line_offset,
        line_content: line.to_owned(),
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

fn find_symbol_in_line(line: &str, symbol_name: &str, order_hint: usize) -> Option<usize> {
    let code = strip_line_comment(line);
    let mut seen = 0usize;
    for (index, _) in code.match_indices(symbol_name) {
        if !has_word_boundaries(code, index, symbol_name.len()) {
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
    text.match_indices(word)
        .any(|(index, _)| has_word_boundaries(text, index, word.len()))
}

fn has_word_boundaries(text: &str, start: usize, len: usize) -> bool {
    let before = if start == 0 {
        None
    } else {
        text[..start].chars().next_back()
    };
    let after = text[start + len..].chars().next();
    !is_ident(before) && !is_ident(after)
}

fn is_ident(ch: Option<char>) -> bool {
    ch.map(|c| c == '_' || c == '$' || c.is_ascii_alphanumeric())
        .unwrap_or(false)
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
