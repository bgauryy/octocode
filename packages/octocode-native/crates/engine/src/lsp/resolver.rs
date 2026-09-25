use crate::error::{Error, Result, Status};
use crate::lsp::grammar::grammar_for_file;
use crate::lsp::types::{JsExactPosition, JsFuzzyPosition, JsResolvedSymbol};
use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use crate::text::utf8_offsets::{byte_to_char_offset_inner, hide_bom_in_line};
use std::fs;
use std::io::Read;
use std::time::Instant;
use tree_sitter::Node;

const DEFAULT_RADIUS: i32 = 5;
const MAX_POSITION_SOURCE_BYTES: usize = 1_000_000;

fn budget_error() -> Error {
    Error::new(
        Status::GenericFailure,
        "[lspPositionTimeout] Symbol position analysis exceeded its time budget; narrow the source.",
    )
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
    /// The match sits inside a string literal (or similar textual node) rather
    /// than an identifier; ranked after identifiers on the same line.
    is_literal: bool,
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

/// [`resolve_position`] over `content` already read for `file_path` (the path
/// only selects the grammar; the file is not read). Lets a caller resolve the
/// anchor on exactly the text it synchronized with `didOpen`.
pub fn resolve_position_in_file_content(
    file_path: &str,
    content: &str,
    fuzzy: &JsFuzzyPosition,
) -> Result<JsResolvedSymbol> {
    check_source_size(content.len())?;
    resolve_position_with_path(file_path, content, fuzzy)
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
    LineIndex::new(content).lines(content)
}

/// Line spans of a text under the LSP definition of a line break: `\r\n`,
/// `\n`, and a lone `\r` each end a line. Built once per text so every lookup
/// (resolver candidates, snippet slices) agrees with the line numbers the
/// server computes, and repeated slices never re-split the text.
///
/// A text ending in a line break has a final empty line, as in LSP.
///
/// Public so the runtime's line counting and position bounds use the same
/// line-break rule as the resolver and snippet reads.
#[derive(Debug)]
pub struct LineIndex {
    /// Byte range of each line, excluding its terminator.
    spans: Vec<(usize, usize)>,
    /// `true` when the text ends with a line break (the last span is empty
    /// and holds no content).
    ends_with_break: bool,
}

impl LineIndex {
    pub fn new(content: &str) -> Self {
        let bytes = content.as_bytes();
        let mut spans = Vec::new();
        let mut start = 0;
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\n' => {
                    spans.push((start, index));
                    index += 1;
                    start = index;
                }
                b'\r' => {
                    spans.push((start, index));
                    index += if bytes.get(index + 1) == Some(&b'\n') {
                        2
                    } else {
                        1
                    };
                    start = index;
                }
                _ => index += 1,
            }
        }
        spans.push((start, bytes.len()));
        Self {
            spans,
            ends_with_break: !bytes.is_empty() && start == bytes.len(),
        }
    }

    /// Number of lines, including the empty last line after a final break.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// Never true: even an empty text has one (empty) line.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Byte offset where `line` starts, or `None` past the last line.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.spans.get(line).map(|&(start, _)| start)
    }

    /// Number of lines that hold content: the final empty line after a
    /// trailing break is not counted (matches `str::lines`).
    pub fn content_len(&self) -> usize {
        self.spans.len() - usize::from(self.ends_with_break)
    }

    /// Text of `line`, without its terminator.
    pub fn line<'a>(&self, content: &'a str, line: usize) -> Option<&'a str> {
        self.spans
            .get(line)
            .and_then(|&(start, end)| content.get(start..end))
    }

    pub fn lines<'a>(&self, content: &'a str) -> Vec<&'a str> {
        (0..self.len())
            .map(|line| self.line(content, line).unwrap_or_default())
            .collect()
    }

    /// `(line, byte column)` of a byte offset. An offset inside a `\r\n`
    /// terminator maps to the end of its line.
    pub fn position_of(&self, byte: usize) -> (usize, usize) {
        let line = self
            .spans
            .partition_point(|&(start, _)| start <= byte)
            .saturating_sub(1);
        let (start, end) = self.spans[line];
        (line, byte.min(end) - start)
    }
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
    let index = LineIndex::new(content);
    collect_symbol_candidates(
        root,
        content,
        &index,
        &fuzzy.symbol_name,
        &mut candidates,
        deadline,
    )?;
    let result = pick_candidate(candidates, fuzzy, lines);
    check_budget(deadline)?;
    Ok(result)
}

/// One step of the walk's root-to-current path: a node's kind and the field
/// it fills in its parent. Kept alongside the cursor so declaration checks
/// never call `Node::parent()` (which re-descends from the root each time).
#[derive(Clone, Copy)]
struct PathStep<'tree> {
    kind: &'tree str,
    field: Option<&'tree str>,
}

fn collect_symbol_candidates(
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

fn pick_candidate(
    mut candidates: Vec<SymbolCandidate>,
    fuzzy: &JsFuzzyPosition,
    lines: &[&str],
) -> Option<JsResolvedSymbol> {
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

fn is_literal_node(kind: &str) -> bool {
    // `template_string` is covered by "string"; C++ `template_*` kinds are
    // identifier-like and deliberately not treated as literals.
    kind.contains("string") || kind.contains("comment")
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
    // A leading BOM is invisible to editors and LSP servers: row-0 columns and
    // line text exclude it, exactly as the AST surfaces report them.
    let (line, character) = hide_bom_in_line(line_index, line, character);
    JsResolvedSymbol {
        position: JsExactPosition {
            line: line_index as u32,
            // `character` arrives as a BYTE offset within `line` (tree-sitter
            // columns and `str::find`/`match_indices` are byte-based). LSP
            // `character` is UTF-16 code units, so convert — otherwise any line
            // with non-ASCII before the symbol mis-positions the cursor.
            character: byte_to_char_offset_inner(line, character) as u32,
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
    let Some(ch) = ch else { return false };
    if ch.is_ascii() {
        return ch == '_' || ch == '$' || ch.is_ascii_alphanumeric();
    }
    // Use a conservative cross-language identifier boundary, including combining
    // marks and ECMAScript join controls. ASCII-only boundaries can bind a
    // requested name to a different Unicode identifier.
    // Constant pattern: compile failure is a build-time bug, so fail loud.
    #[allow(clippy::expect_used)]
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
    use super::{LineIndex, resolve_position_with_path};
    use crate::lsp::types::JsFuzzyPosition;

    fn fuzzy(name: &str, line_hint: Option<u32>) -> JsFuzzyPosition {
        JsFuzzyPosition {
            symbol_name: name.to_owned(),
            line_hint,
            order_hint: None,
        }
    }

    #[test]
    fn line_index_breaks_on_crlf_lf_and_lone_cr() {
        let text = "a\r\nb\rc\nd";
        let index = LineIndex::new(text);
        assert_eq!(index.lines(text), ["a", "b", "c", "d"]);
        assert_eq!(index.content_len(), 4);
        assert_eq!(index.position_of(text.find('c').unwrap()), (2, 0));
        assert_eq!(index.position_of(text.find('d').unwrap()), (3, 0));

        // A trailing break yields an empty last line that holds no content.
        let text = "x\r\ny\r";
        let index = LineIndex::new(text);
        assert_eq!(index.lines(text), ["x", "y", ""]);
        assert_eq!(index.len(), 3);
        assert_eq!(index.content_len(), 2);
        assert_eq!(LineIndex::new("").lines(""), [""]);
        assert_eq!(LineIndex::new("").content_len(), 1);
    }

    #[test]
    fn lexical_resolution_counts_crlf_and_lone_cr_as_line_breaks() {
        for source in [
            "let a = 1;\r\nlet b = 2;\r\nfn target() {}\r\n",
            "let a = 1;\rlet b = 2;\rfn target() {}\r",
            "let a = 1;\nlet b = 2;\r\nfn target() {}\r",
        ] {
            let hit =
                super::resolve_position_from_content(source.to_owned(), fuzzy("target", None))
                    .expect("target resolves");
            assert_eq!(hit.position.line, 2, "{source:?}");
            assert_eq!(hit.position.character, 3, "{source:?}");
            assert_eq!(hit.line_content, "fn target() {}");
        }
    }

    #[test]
    fn grammar_resolution_counts_lone_cr_as_a_line_break() {
        // Tree-sitter rows count only `\n`; with lone `\r` breaks the whole
        // file is one tree-sitter row, but the server sees three lines.
        let source = "const a = 1;\rconst b = 2;\rfunction target() {}\r";
        let hit = resolve_position_with_path("demo.ts", source, &fuzzy("target", None))
            .expect("target resolves");
        assert_eq!(hit.position.line, 2);
        assert_eq!(hit.position.character, 9);
        assert_eq!(hit.found_at_line, 3);
        assert_eq!(hit.line_content, "function target() {}");

        let crlf = "const a = 1;\r\nfunction target() {}\r\n";
        let hit = resolve_position_with_path("demo.ts", crlf, &fuzzy("target", Some(2)))
            .expect("target resolves");
        assert_eq!((hit.position.line, hit.position.character), (1, 9));
    }

    #[test]
    fn grammar_resolution_reports_utf16_columns_after_non_ascii_and_emoji() {
        // "é" is 2 UTF-8 bytes / 1 UTF-16 unit; "😀" is 4 bytes / 2 units.
        let source = "const a = 1;\r\nconst s = 'é😀'; const target = 2;\r\n";
        let hit = resolve_position_with_path("demo.ts", source, &fuzzy("target", Some(2)))
            .expect("target resolves");
        assert_eq!(hit.position.line, 1);
        let expected = "const s = 'é😀'; const ".encode_utf16().count() as u32;
        assert_eq!(hit.position.character, expected);

        let lexical = super::resolve_position_from_content(
            "x\ré😀 target\n".to_owned(),
            fuzzy("target", Some(2)),
        )
        .expect("lexical target");
        assert_eq!((lexical.position.line, lexical.position.character), (1, 4));
    }

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
            &super::LineIndex::new(source),
            "target",
            &mut candidates,
            std::time::Instant::now(),
        );
        assert!(
            result
                .expect_err("walk budget must fail")
                .reason
                .contains("[lspPositionTimeout]")
        );
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

    fn resolve_with_order(source: &str, symbol_name: &str, line_hint: u32, order: u32) -> u32 {
        resolve_position_with_path(
            "demo.ts",
            source,
            &JsFuzzyPosition {
                symbol_name: symbol_name.to_owned(),
                line_hint: Some(line_hint),
                order_hint: Some(order),
            },
        )
        .unwrap_or_else(|err| panic!("failed to resolve {symbol_name} order {order}: {err}"))
        .position
        .character
    }

    #[test]
    fn order_hint_skips_duplicate_string_nodes_and_prefers_identifiers() {
        // `"foo"` yields a `string` node and a `string_fragment` child that both
        // point at column 11; they must collapse into one candidate, and the
        // real identifiers must be ranked before the string-literal mention.
        let source = "const x = \"foo\"; foo(); foo();\n";
        assert_eq!(resolve_with_order(source, "foo", 1, 0), 17);
        assert_eq!(resolve_with_order(source, "foo", 1, 1), 24);
        assert_eq!(resolve_with_order(source, "foo", 1, 2), 11);
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
            (
                "demo.cu",
                "__global__ void target() {}\nvoid launch() { target<<<1, 1>>>(); }\n",
                1,
            ),
            ("demo.asm", "target:\n  mov %rax, %rbx\n", 1),
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

    fn resolve_line(
        file_name: &str,
        source: &str,
        symbol_name: &str,
        line_hint: Option<u32>,
    ) -> u32 {
        resolve_position_with_path(file_name, source, &fuzzy(symbol_name, line_hint))
            .unwrap_or_else(|err| panic!("{file_name}: {err}"))
            .found_at_line
    }

    #[test]
    fn uses_inside_class_and_function_bodies_are_not_declarations() {
        // Without a lineHint the declaration wins over earlier uses; a use in a
        // class body, method body or object value must not count as one.
        let cases = [
            (
                "demo.ts",
                "class Service {\n  run() { return helper(); }\n}\nfunction helper() { return 1; }\n",
                4,
            ),
            (
                "demo.js",
                "const routes = { run: () => helper() };\nfunction helper() {}\n",
                2,
            ),
            (
                "demo.tsx",
                "class View {\n  render() { return <div>{helper()}</div>; }\n}\nconst helper = () => 1;\n",
                4,
            ),
            (
                "demo.rs",
                "impl S {\n    fn run(&self) { helper(); }\n}\nfn helper() {}\n",
                4,
            ),
            (
                "demo.py",
                "class A:\n    def run(self):\n        return helper()\n\ndef helper():\n    pass\n",
                5,
            ),
            (
                "demo.go",
                "package main\ntype S struct{}\nfunc (s S) Run() { helper() }\nfunc helper() {}\n",
                4,
            ),
            (
                "demo.java",
                "class A {\n  void run() { helper(); }\n  void helper() {}\n}\n",
                3,
            ),
        ];
        for (file_name, source, expected) in cases {
            assert_eq!(
                resolve_line(file_name, source, "helper", None),
                expected,
                "{file_name}"
            );
        }
    }

    #[test]
    fn class_method_field_and_variable_names_are_declarations() {
        let cases = [
            // method name
            (
                "demo.ts",
                "service.helper();\nclass Service { helper() {} }\n",
                "helper",
                2,
            ),
            // class field
            (
                "demo.ts",
                "use(obj.count);\nclass C { count = 0; }\n",
                "count",
                2,
            ),
            // class name
            (
                "demo.ts",
                "new Service();\nclass Service {}\n",
                "Service",
                2,
            ),
            // interface member
            (
                "demo.ts",
                "x.size();\ninterface Box { size(): number }\n",
                "size",
                2,
            ),
            // object literal key
            (
                "demo.js",
                "use(cfg.port);\nconst cfg = { port: 1 };\n",
                "port",
                2,
            ),
            // Rust struct field + struct name
            (
                "demo.rs",
                "fn f(s: S) -> u8 { s.count }\nstruct S { count: u8 }\n",
                "count",
                2,
            ),
            ("demo.rs", "fn f(s: Store) {}\nstruct Store;\n", "Store", 2),
            // Python method + assignment
            (
                "demo.py",
                "obj.run()\nclass A:\n    def run(self):\n        pass\n",
                "run",
                3,
            ),
            ("demo.py", "print(limit)\nlimit = 3\n", "limit", 2),
            // Go short var declaration through an expression_list
            (
                "demo.go",
                "package main\nfunc f() {\n  use(total)\n  total, n := 1, 2\n}\n",
                "total",
                4,
            ),
            // C function declarator
            (
                "demo.c",
                "int main() { return target(); }\nint target() { return 1; }\n",
                "target",
                2,
            ),
        ];
        for (file_name, source, symbol, expected) in cases {
            assert_eq!(
                resolve_line(file_name, source, symbol, None),
                expected,
                "{file_name}: {symbol}"
            );
        }
    }

    #[test]
    fn line_hint_prefers_the_declaration_over_an_equally_near_use() {
        // Line 3 is the hint; the use (line 2, inside a method body) and the
        // declaration (line 4) are equally near, so the declaration must win.
        let source = "class A {\n  run() { return helper(); }\n}\nfunction helper() {}\n";
        let hit = resolve_position_with_path("demo.ts", source, &fuzzy("helper", Some(3)))
            .expect("resolves");
        assert_eq!(hit.found_at_line, 4);
        assert_eq!(hit.position.character, 9);
        // An exact hit on the hinted line still wins over a nearby declaration.
        assert_eq!(resolve_line("demo.ts", source, "helper", Some(2)), 2);
    }

    #[test]
    fn leading_bom_is_hidden_from_row_zero_columns_and_text() {
        for file_name in ["demo.ts", "demo.unknown"] {
            let source = "\u{feff}const target = 1;\nconst other = target;\n";
            let hit = resolve_position_with_path(file_name, source, &fuzzy("target", Some(1)))
                .expect("resolves");
            assert_eq!(
                (hit.position.line, hit.position.character),
                (0, 6),
                "{file_name}"
            );
            assert_eq!(hit.line_content, "const target = 1;", "{file_name}");
            // Later rows are unaffected by the BOM.
            let hit = resolve_position_with_path(file_name, source, &fuzzy("target", Some(2)))
                .expect("resolves");
            assert_eq!(
                (hit.position.line, hit.position.character),
                (1, 14),
                "{file_name}"
            );
        }
        // A symbol at column 0 right after the BOM.
        let hit =
            resolve_position_with_path("demo.py", "\u{feff}target = 1\n", &fuzzy("target", None))
                .expect("resolves");
        assert_eq!((hit.position.line, hit.position.character), (0, 0));
    }
}
