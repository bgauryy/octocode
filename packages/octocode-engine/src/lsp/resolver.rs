use crate::lsp::grammar::grammar_for_file;
use crate::lsp::types::{JsExactPosition, JsFuzzyPosition, JsResolvedSymbol};
use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use napi::{Error, Result, Status};
use std::fs;
use std::io::Read;
use std::time::Instant;
use tree_sitter::Node;

const DEFAULT_RADIUS: i32 = 5;
const MAX_POSITION_SOURCE_BYTES: usize = 1_000_000;

fn budget_error() -> Error {
    Error::new(Status::GenericFailure, "[lspPositionTimeout] Symbol position analysis exceeded its time budget; narrow the source.")
}

fn check_budget(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(budget_error())
    } else {
        Ok(())
    }
}

fn check_source_size(size: usize) -> Result<()> {
    if size > MAX_POSITION_SOURCE_BYTES {
        Err(Error::new(
            Status::GenericFailure,
            "[lspSourceTooLarge] Symbol position source exceeds 1000000 bytes; narrow the source.",
        ))
    } else {
        Ok(())
    }
}

#[derive(Clone)]
struct SymbolCandidate {
    line_index: usize,
    character: usize,
    is_exact: bool,
    is_declaration: bool,
}

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
    let metadata = fs::metadata(&file_path).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to read {file_path}: {err}"),
        )
    })?;
    if !metadata.is_file() {
        return Err(Error::new(
            Status::InvalidArg,
            "Symbol position source must be a regular file",
        ));
    }
    check_source_size(metadata.len().try_into().unwrap_or(usize::MAX))?;
    let file = fs::File::open(&file_path).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to read {file_path}: {err}"),
        )
    })?;
    let metadata = file
        .metadata()
        .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))?;
    if !metadata.is_file() {
        return Err(Error::new(
            Status::InvalidArg,
            "Symbol position source must be a regular file",
        ));
    }
    check_source_size(metadata.len().try_into().unwrap_or(usize::MAX))?;
    let mut content = String::new();
    file.take((MAX_POSITION_SOURCE_BYTES + 1) as u64)
        .read_to_string(&mut content)
        .map_err(|err| {
            Error::new(
                Status::GenericFailure,
                format!("Failed to read {file_path}: {err}"),
            )
        })?;
    resolve_position_with_path(&file_path, &content, &fuzzy)
}

pub fn resolve_position_from_content(
    content: String,
    fuzzy: JsFuzzyPosition,
) -> Result<JsResolvedSymbol> {
    check_source_size(content.len())?;
    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
    let lines = normalized_lines(&content);
    resolve_position_from_lines(&lines, &fuzzy, deadline)
}

fn resolve_position_with_path(
    file_path: &str,
    content: &str,
    fuzzy: &JsFuzzyPosition,
) -> Result<JsResolvedSymbol> {
    resolve_position_before(
        file_path,
        content,
        fuzzy,
        Instant::now() + AST_EXECUTION_TIMEOUT,
    )
}

fn resolve_position_before(
    file_path: &str,
    content: &str,
    fuzzy: &JsFuzzyPosition,
    deadline: Instant,
) -> Result<JsResolvedSymbol> {
    check_source_size(content.len())?;
    check_budget(deadline)?;
    let lines = normalized_lines(content);
    if let Some(hit) = resolve_position_with_grammar(file_path, content, &lines, fuzzy, deadline)? {
        return Ok(hit);
    }
    resolve_position_from_lines(&lines, fuzzy, deadline)
}

fn resolve_position_from_lines(
    lines: &[&str],
    fuzzy: &JsFuzzyPosition,
    deadline: Instant,
) -> Result<JsResolvedSymbol> {
    check_budget(deadline)?;
    let order_hint = fuzzy.order_hint.unwrap_or(0) as usize;

    match fuzzy.line_hint {
        None | Some(0) => scan_whole_file(lines, &fuzzy.symbol_name, order_hint, deadline)?
            .ok_or_else(|| {
                Error::new(
                    Status::GenericFailure,
                    format!(
                        "Could not find symbol '{}' anywhere in the file",
                        fuzzy.symbol_name
                    ),
                )
            }),
        Some(line_hint) => {
            let result = scan_near_line(lines, &fuzzy.symbol_name, line_hint, order_hint);
            check_budget(deadline)?;
            result
        }
    }
}

fn normalized_lines(content: &str) -> Vec<&str> {
    let lines: Vec<&str> = content
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    lines
}

fn resolve_position_with_grammar(
    file_path: &str,
    content: &str,
    lines: &[&str],
    fuzzy: &JsFuzzyPosition,
    deadline: Instant,
) -> Result<Option<JsResolvedSymbol>> {
    let Some(spec) = grammar_for_file(file_path) else {
        return Ok(None);
    };
    let tree = spec
        .parse_before(content, deadline)
        .ok_or_else(budget_error)?;
    let root = tree.root_node();
    if root.has_error() {
        return Ok(None);
    }

    let mut candidates = Vec::new();
    collect_symbol_candidates(root, content, &fuzzy.symbol_name, &mut candidates, deadline)?;
    let result = pick_candidate(candidates, fuzzy, lines);
    check_budget(deadline)?;
    Ok(result)
}

fn collect_symbol_candidates(
    node: Node<'_>,
    content: &str,
    symbol_name: &str,
    candidates: &mut Vec<SymbolCandidate>,
    deadline: Instant,
) -> Result<()> {
    let mut cursor = node.walk();
    loop {
        check_budget(deadline)?;
        let current = cursor.node();
        if current.is_named() && !is_ignored_node(current.kind()) {
            if let Some(candidate) = candidate_from_node(current, content, symbol_name) {
                candidates.push(candidate);
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(());
            }
        }
    }
}

fn candidate_from_node(
    node: Node<'_>,
    content: &str,
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
    let position = node.start_position();
    Some(SymbolCandidate {
        line_index: position.row,
        character: position.column + match_offset,
        is_exact,
        is_declaration: looks_like_declaration_node(node),
    })
}

fn pick_candidate(
    mut candidates: Vec<SymbolCandidate>,
    fuzzy: &JsFuzzyPosition,
    lines: &[&str],
) -> Option<JsResolvedSymbol> {
    candidates.sort_by_key(|candidate| (candidate.line_index, candidate.character));
    let order_hint = fuzzy.order_hint.unwrap_or(0) as usize;

    let selected = match fuzzy.line_hint {
        Some(line_hint) if line_hint > 0 => {
            let target = line_hint as i32 - 1;
            let mut same_line: Vec<&SymbolCandidate> = candidates
                .iter()
                .filter(|candidate| candidate.line_index as i32 == target)
                .collect();
            same_line.sort_by_key(|candidate| candidate.character);
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
                !candidate.is_exact,
                candidate.line_index,
                candidate.character,
            )
        }),
    }?;

    Some(hit_for(
        lines.get(selected.line_index).copied().unwrap_or_default(),
        selected.line_index,
        selected.character,
        fuzzy
            .line_hint
            .filter(|line_hint| *line_hint > 0)
            .map(|line_hint| selected.line_index as i32 - (line_hint as i32 - 1))
            .unwrap_or(0),
    ))
}

fn is_ignored_node(kind: &str) -> bool {
    kind.contains("comment") || kind == "ERROR"
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

fn looks_like_declaration_node(node: Node<'_>) -> bool {
    let kind = node.kind();
    if is_declaration_kind(kind) {
        return true;
    }

    let mut parent = node.parent();
    while let Some(current) = parent {
        if is_declaration_kind(current.kind()) {
            return true;
        }
        parent = current.parent();
    }
    false
}

fn is_declaration_kind(kind: &str) -> bool {
    kind.contains("declaration")
        || kind.contains("definition")
        || kind.contains("function_item")
        || kind.contains("function_declarator")
        || kind.contains("method")
        || kind.contains("class")
        || kind.contains("struct")
        || kind.contains("interface")
        || kind.contains("type_alias")
        || kind.contains("lexical_declaration")
        || kind.contains("variable_declaration")
        || kind.contains("pair")
        || kind.contains("selector")
        || kind == "assignment"
}

fn exact_symbol_text(text: &str, symbol_name: &str) -> bool {
    if text == symbol_name {
        return true;
    }
    text.trim_matches(['"', '\'', '`'])
        .trim_start_matches(['.', '#', '$', '@'])
        == symbol_name
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
    deadline: Instant,
) -> Result<Option<JsResolvedSymbol>> {
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

fn hit_for(line: &str, line_index: usize, character: usize, line_offset: i32) -> JsResolvedSymbol {
    JsResolvedSymbol {
        position: JsExactPosition {
            line: line_index as u32,
            // `character` arrives as a BYTE offset within `line` (tree-sitter
            // columns and `str::find`/`match_indices` are byte-based). LSP
            // `character` is UTF-16 code units, so convert — otherwise any line
            // with non-ASCII before the symbol mis-positions the cursor.
            character: byte_offset_to_utf16(line, character),
        },
        found_at_line: line_index as u32 + 1,
        line_offset,
        line_content: line.to_owned(),
    }
}

/// Convert a byte offset within `line` to a UTF-16 code-unit offset (the LSP
/// `character` unit). Counts every char whose start byte precedes `byte_offset`.
fn byte_offset_to_utf16(line: &str, byte_offset: usize) -> u32 {
    let mut bytes = 0usize;
    let mut utf16 = 0u32;
    for ch in line.chars() {
        if bytes >= byte_offset {
            break;
        }
        bytes += ch.len_utf8();
        utf16 += ch.len_utf16() as u32;
    }
    utf16
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
    let Some(ch) = ch else { return false };
    if ch.is_ascii() {
        return ch == '_' || ch == '$' || ch.is_ascii_alphanumeric();
    }
    // Use a conservative cross-language identifier boundary, including combining
    // marks and ECMAScript join controls. ASCII-only boundaries can bind a
    // requested name to a different Unicode identifier.
    static IDENT_CONTINUE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^[\p{ID_Continue}\u{200C}\u{200D}]$")
            .expect("valid Unicode identifier boundary")
    });
    IDENT_CONTINUE.is_match(ch.encode_utf8(&mut [0; 4]))
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

#[cfg(test)]
mod tests {
    use super::{byte_offset_to_utf16, resolve_position_with_path};
    use crate::lsp::types::JsFuzzyPosition;

    #[test]
    fn oversized_position_source_returns_limit_instead_of_a_symbol() {
        let source = format!("const target = 1;\n{}", " ".repeat(1_000_000));
        let result = resolve_position_with_path(
            "demo.ts",
            &source,
            &JsFuzzyPosition {
                symbol_name: "target".to_owned(),
                line_hint: Some(1),
                order_hint: None,
            },
        );
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("oversized source must be bounded"),
        };
        assert!(
            error.reason.contains("[lspSourceTooLarge]"),
            "{}",
            error.reason
        );
    }

    #[test]
    fn expired_position_budget_does_not_fall_back_to_a_lexical_hit() {
        for file in ["demo.ts", "demo.unknown"] {
            let result = super::resolve_position_before(
                file,
                "const target = 1;",
                &JsFuzzyPosition {
                    symbol_name: "target".to_owned(),
                    line_hint: Some(1),
                    order_hint: None,
                },
                std::time::Instant::now(),
            );
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("expired analysis must fail"),
            };
            assert!(error.reason.contains("[lspPositionTimeout]"));
        }
    }

    #[test]
    fn expired_walk_budget_discards_partial_candidates() {
        let source = "const target = 1;";
        let spec = super::grammar_for_file("demo.ts").unwrap();
        let tree = spec
            .parse_before(
                source,
                std::time::Instant::now() + super::AST_EXECUTION_TIMEOUT,
            )
            .unwrap();
        let mut candidates = Vec::new();
        let result = super::collect_symbol_candidates(
            tree.root_node(),
            source,
            "target",
            &mut candidates,
            std::time::Instant::now(),
        );
        assert!(result
            .expect_err("walk budget must fail")
            .reason
            .contains("[lspPositionTimeout]"));
        assert!(candidates.is_empty());
    }

    #[test]
    fn deeply_nested_position_walk_is_bounded_without_recursive_stack_growth() {
        let source = format!(
            "const target = {}1{};",
            "(".repeat(10_000),
            ")".repeat(10_000)
        );
        let result = super::resolve_position_with_path(
            "demo.ts",
            &source,
            &JsFuzzyPosition {
                symbol_name: "target".to_owned(),
                line_hint: Some(1),
                order_hint: None,
            },
        );
        match result {
            Ok(hit) => assert_eq!(hit.position.character, 6),
            Err(error) => assert!(
                error.reason.contains("[lspPositionTimeout]"),
                "{}",
                error.reason
            ),
        }
    }

    #[test]
    fn byte_offset_converts_to_utf16_character() {
        // 'é' = 2 UTF-8 bytes / 1 UTF-16 unit: "target" starts at byte 8, char 7.
        assert_eq!(byte_offset_to_utf16("const étarget = 1;", 8), 7);
        // '🌍' = 4 UTF-8 bytes / 2 UTF-16 units (surrogate pair): byte 10, char 8.
        assert_eq!(byte_offset_to_utf16("const 🌍target = 1;", 10), 8);
        assert_eq!(byte_offset_to_utf16("ascii only", 6), 6);
        assert_eq!(byte_offset_to_utf16("", 0), 0);
    }

    fn resolve_char(file_name: &str, source: &str, symbol_name: &str, line_hint: u32) -> u32 {
        resolve_position_with_path(
            file_name,
            source,
            &JsFuzzyPosition {
                symbol_name: symbol_name.to_owned(),
                line_hint: Some(line_hint),
                order_hint: None,
            },
        )
        .expect("must resolve")
        .position
        .character
    }

    #[test]
    fn resolves_utf16_character_for_non_ascii_prefix() {
        // The prefix contains both a BMP character and a surrogate pair.
        let prefix = "const label = 'é🌍'; const ";
        assert_eq!(
            resolve_char("demo.ts", &format!("{prefix}target = 1;\n"), "target", 1),
            prefix.encode_utf16().count() as u32
        );
    }

    #[test]
    fn unicode_identifier_substrings_are_not_symbol_anchors() {
        for identifier in [
            "étarget",
            "targeté",
            "target\u{301}",
            "℘target",
            "target\u{200c}tail",
            "東京target",
            "𐐀target",
        ] {
            let source = format!("const {identifier} = 1;\n");
            for file_name in ["demo.ts", "demo.unknown"] {
                assert!(
                    resolve_position_with_path(
                        file_name,
                        &source,
                        &JsFuzzyPosition {
                            symbol_name: "target".to_owned(),
                            line_hint: Some(1),
                            order_hint: None,
                        }
                    )
                    .is_err(),
                    "must not resolve target inside {identifier} in {file_name}"
                );
            }
        }
    }

    #[test]
    fn complete_unicode_identifiers_keep_utf16_positions() {
        let prefix = "const label = 'é🌍'; const ";
        for identifier in [
            "étarget",
            "targeté",
            "target\u{301}",
            "℘target",
            "target\u{200c}tail",
            "東京target",
            "𐐀target",
            "$target",
        ] {
            let source = format!("{prefix}{identifier} = 1;\n");
            for file_name in ["demo.ts", "demo.unknown"] {
                assert_eq!(
                    resolve_char(file_name, &source, identifier, 1),
                    prefix.encode_utf16().count() as u32,
                    "{identifier} in {file_name}"
                );
            }
        }
    }

    fn resolve(file_name: &str, source: &str, symbol_name: &str, line_hint: u32) -> u32 {
        let result = resolve_position_with_path(
            file_name,
            source,
            &JsFuzzyPosition {
                symbol_name: symbol_name.to_owned(),
                line_hint: Some(line_hint),
                order_hint: None,
            },
        );
        match result {
            Ok(hit) => hit.found_at_line,
            Err(err) => panic!("failed to resolve {symbol_name} in {file_name}: {err}"),
        }
    }

    #[test]
    fn tree_sitter_anchor_ignores_comment_on_requested_line() {
        let source = "/* target is mentioned in a block comment */\nfunction target() {}\n";
        assert_eq!(resolve("demo.ts", source, "target", 1), 2);
    }

    #[test]
    fn tree_sitter_resolves_requested_language_matrix() {
        let cases = [
            ("demo.ts", "export function target() {}\n", 1),
            ("demo.tsx", "export const target = () => <div />;\n", 1),
            ("demo.js", "export function target() {}\n", 1),
            ("demo.jsx", "export const target = () => <div />;\n", 1),
            ("demo.py", "def target():\n    return 1\n", 1),
            ("demo.go", "package main\nfunc target() {}\n", 2),
            ("demo.rs", "fn target() {}\n", 1),
            ("demo.java", "class Target { void target() {} }\n", 1),
            ("demo.c", "void target() {}\n", 1),
            ("demo.cpp", "void target() {}\n", 1),
            ("demo.cs", "class Target { void target() {} }\n", 1),
            ("demo.sh", "target() { echo ok; }\n", 1),
            ("demo.json", "{\"target\": true}\n", 1),
            ("demo.yaml", "target: true\n", 1),
            ("demo.toml", "target = true\n", 1),
            ("demo.html", "<div id=\"target\"></div>\n", 1),
            ("demo.css", ".target { color: red; }\n", 1),
            ("demo.scss", ".target { color: red; }\n", 1),
            ("demo.less", ".target { color: red; }\n", 1),
        ];

        for (file_name, source, expected_line) in cases {
            assert_eq!(
                resolve(file_name, source, "target", 1),
                expected_line,
                "{file_name}"
            );
        }
    }
}
