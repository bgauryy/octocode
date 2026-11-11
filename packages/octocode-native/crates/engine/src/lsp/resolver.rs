use crate::error::{Error, Result};
use crate::lsp::grammar::grammar_for_file;
use crate::lsp::types::{LspFuzzyPosition, LspResolvedSymbol};
use crate::signatures::extractor::whole_file_deadline;
use std::time::Instant;

mod line_scan;
mod syntax;

use line_scan::{scan_near_line, scan_whole_file};
use syntax::{collect_symbol_candidates, pick_candidate};

/// How many lines from its `line_hint` the resolver accepts a spelling: it
/// chooses the nearest among the lines within this distance.
pub const DEFAULT_RADIUS: usize = 5;

/// The 0-based line a 1-based `line_hint` names (0 and 1 both name line 0).
fn hinted_line(line_hint: u32) -> usize {
    usize::try_from(line_hint.saturating_sub(1)).unwrap_or(usize::MAX)
}

/// Signed line distance `to - from`, saturating at the `i32` range.
fn line_delta(from: usize, to: usize) -> i32 {
    if to >= from {
        i32::try_from(to - from).unwrap_or(i32::MAX)
    } else {
        i32::try_from(from - to).map_or(i32::MIN, |distance| -distance)
    }
}
const MAX_POSITION_SOURCE_BYTES: u64 = super::MAX_LSP_SOURCE_BYTES;

/// Whether a `len`-byte source is past the position-resolution cap.
fn exceeds_source_cap(len: usize) -> bool {
    u64::try_from(len).map_or(true, |len| len > MAX_POSITION_SOURCE_BYTES)
}

fn budget_error() -> Error {
    Error::new(
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
    if exceeds_source_cap(size) {
        Err(Error::new(format!(
            "[lspSourceTooLarge] Symbol position source exceeds {MAX_POSITION_SOURCE_BYTES} bytes; narrow the source."
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn resolve_position_from_content(
    content: String,
    fuzzy: LspFuzzyPosition,
) -> Result<LspResolvedSymbol> {
    check_source_size(content.len())?;
    let deadline = whole_file_deadline();
    let lines = normalized_lines(&content);
    resolve_position_from_lines(&lines, &fuzzy, deadline)
}

/// [`resolve_position_from_content`] over `content` already read for `file_path` (the path
/// only selects the grammar; the file is not read). Lets a caller resolve the
/// anchor on exactly the text it synchronized with `didOpen`.
pub fn resolve_position_in_file_content(
    file_path: &str,
    content: &str,
    fuzzy: &LspFuzzyPosition,
) -> Result<LspResolvedSymbol> {
    check_source_size(content.len())?;
    resolve_position_before(file_path, content, fuzzy, whole_file_deadline())
}

/// Zero-based lines where `symbol_name` occurs as code in `content`: an
/// exact identifier-like node, not inside a comment or a string literal.
/// The path selects the grammar (the file is not read). `None` when the
/// file has no grammar, or its parse fails, has errors, or exceeds the
/// analysis budget: the caller cannot tell code from comments then.
#[must_use]
pub fn code_occurrence_lines(
    file_path: &str,
    content: &str,
    symbol_name: &str,
) -> Option<Vec<usize>> {
    code_occurrences(file_path, content, symbol_name).map(|occurrences| occurrences.lines)
}

/// Zero-based first and last lines of the scope of the local binding (a
/// parameter no call site names by keyword, or a variable bound inside a
/// callable) that `symbol_name` at
/// zero-based `line` and UTF-16 `character` of `content` names: no text
/// outside those lines, and none in another file, can name it. The path
/// selects the grammar (the file is not read). `None` when the name there
/// is no local binding, or the grammar cannot tell (no grammar, a parse with
/// errors, a closure or macro around it, the analysis budget).
#[must_use]
pub fn local_binding_lines(
    file_path: &str,
    content: &str,
    symbol_name: &str,
    line: u32,
    character: u32,
) -> Option<(usize, usize)> {
    if exceeds_source_cap(content.len()) || symbol_name.is_empty() {
        return None;
    }
    let at = LineIndex::new(content).position_to_byte(content, line, character) as usize;
    let scope = crate::signatures::local_binding_scope(
        content,
        file_path,
        symbol_name,
        at,
        whole_file_deadline(),
    )?;
    let line_of = |offset| crate::text::utf8_offsets::line_containing(content, offset) - 1;
    Some((line_of(scope.start), line_of(scope.end.saturating_sub(1))))
}

/// [`code_occurrence_lines`] with the parse that found them, which
/// [`CodeOccurrences::sites`] reads rather than parse again.
#[must_use]
pub fn code_occurrences(
    file_path: &str,
    content: &str,
    symbol_name: &str,
) -> Option<CodeOccurrences> {
    if exceeds_source_cap(content.len()) || symbol_name.is_empty() {
        return None;
    }
    let deadline = whole_file_deadline();
    let (tree, language) = parse_as_code(file_path, content, deadline)?;
    let root = tree.root_node();
    let mut candidates = Vec::new();
    let index = LineIndex::new(content);
    collect_symbol_candidates(
        root,
        content,
        &index,
        symbol_name,
        &mut candidates,
        deadline,
    )
    .ok()?;
    let mut lines = candidates
        .into_iter()
        .filter(|candidate| candidate.is_exact && !candidate.is_literal)
        .map(|candidate| candidate.line_index)
        .collect::<Vec<_>>();
    lines.dedup();
    Some(CodeOccurrences {
        lines,
        tree,
        language,
    })
}

/// Where a name occurs as code in one text.
pub struct CodeOccurrences {
    /// Zero-based lines, see [`code_occurrence_lines`].
    pub lines: Vec<usize>,
    tree: tree_sitter::Tree,
    language: &'static tree_sitter::Language,
}

impl CodeOccurrences {
    /// Where `name` is declared and called in `content` (the text these
    /// occurrences were found in), as [`crate::signatures::extract_graph_facts`]
    /// reads the file; `None` when only those facts can tell. Reads only the
    /// parts of the file spelling the name, so it costs far less than the
    /// facts on a large file.
    #[must_use]
    pub fn sites(
        &self,
        file_path: &str,
        content: &str,
        name: &str,
    ) -> Option<crate::signatures::graph_facts::NamedSites> {
        use crate::signatures::{extractor, graph_facts, graph_facts_grammar};
        let (entry, grammar) = graph_facts_grammar(content, file_path)?;
        let own_parse;
        let tree = if std::ptr::eq(&entry.language, self.language) {
            &self.tree
        } else {
            own_parse = extractor::parse_before(content, &entry.language, whole_file_deadline())?;
            &own_parse
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            graph_facts::named_sites_from_tree(content, file_path, &grammar, tree, name)
        }))
        .unwrap_or(None)
    }
}

fn resolve_position_before(
    file_path: &str,
    content: &str,
    fuzzy: &LspFuzzyPosition,
    deadline: Instant,
) -> Result<LspResolvedSymbol> {
    check_source_size(content.len())?;
    check_budget(deadline)?;
    let lines = normalized_lines(content);
    if let Some(hit) = resolve_position_with_grammar(file_path, content, &lines, fuzzy, deadline)? {
        return Ok(hit);
    }
    resolve_position_from_lines(&lines, fuzzy, deadline).or_else(|error| {
        // A miss near the line says where the symbol occurs: the agent's next
        // `line`. As code when the grammar can tell; a file no grammar reads
        // has only its text, where one site is the one meant.
        match code_occurrence_lines(file_path, content, &fuzzy.symbol_name) {
            Some(found) => Err(name_lines(error, "as code", &found)),
            None if Instant::now() >= deadline => Err(error),
            None => only_text_site(&lines, fuzzy)
                .ok_or_else(|| found_in_text(&lines, fuzzy))
                .map_err(|found| name_lines(error, "in the file's text", &found)),
        }
    })
}

/// Zero-based lines that spell the name outside a line comment.
fn found_in_text(lines: &[&str], fuzzy: &LspFuzzyPosition) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line_scan::find_symbol_in_line(line, &fuzzy.symbol_name, 0).is_some())
        .map(|(index, _)| index)
        .collect()
}

/// The hit of the only line of the file's text that spells the name, for a
/// `line` too far from it; an explicit `occurrence` counts on the line it
/// named, so it means nothing here.
fn only_text_site(lines: &[&str], fuzzy: &LspFuzzyPosition) -> Option<LspResolvedSymbol> {
    let hint = fuzzy.line_hint.filter(|hint| *hint > 0)?;
    if fuzzy.order_hint.unwrap_or(0) > 0 {
        return None;
    }
    let [only] = found_in_text(lines, fuzzy)[..] else {
        return None;
    };
    let character = line_scan::find_symbol_in_line(lines[only], &fuzzy.symbol_name, 0)?;
    Some(line_scan::hit_for(
        lines[only],
        only,
        character,
        line_delta(hinted_line(hint), only),
    ))
}

fn resolve_position_from_lines(
    lines: &[&str],
    fuzzy: &LspFuzzyPosition,
    deadline: Instant,
) -> Result<LspResolvedSymbol> {
    check_budget(deadline)?;
    let order_hint = fuzzy.order_hint.unwrap_or(0) as usize;

    match fuzzy.line_hint {
        None | Some(0) => scan_whole_file(lines, &fuzzy.symbol_name, order_hint, deadline)?
            .ok_or_else(|| {
                Error::new(format!(
                    "Could not find symbol '{}' anywhere in the file",
                    fuzzy.symbol_name
                ))
            }),
        Some(line_hint) => {
            let result = scan_near_line(lines, &fuzzy.symbol_name, line_hint, order_hint);
            check_budget(deadline)?;
            result
        }
    }
}

use crate::text::LineIndex;

/// `content` parsed without errors by the grammar owning `file_path`'s
/// extension, else by the grammar that also parses it (a C++ header under
/// `.h`): a tree the resolver can tell code from comments with. `None` when
/// no grammar applies, every parse has errors, or the budget runs out.
fn parse_as_code(
    file_path: &str,
    content: &str,
    deadline: Instant,
) -> Option<(tree_sitter::Tree, &'static tree_sitter::Language)> {
    let owner = grammar_for_file(file_path)?;
    let tree = owner.parse_before(content, deadline)?;
    if !tree.root_node().has_error() {
        return Some((tree, owner.language()));
    }
    let sibling = crate::lsp::grammar::sibling_grammar_for_file(file_path)?;
    let tree = sibling
        .parse_before(content, deadline)
        .filter(|tree| !tree.root_node().has_error())?;
    Some((tree, sibling.language()))
}

fn normalized_lines(content: &str) -> Vec<&str> {
    LineIndex::new(content).lines(content)
}

fn resolve_position_with_grammar(
    file_path: &str,
    content: &str,
    lines: &[&str],
    fuzzy: &LspFuzzyPosition,
    deadline: Instant,
) -> Result<Option<LspResolvedSymbol>> {
    if grammar_for_file(file_path).is_none() {
        return Ok(None);
    }
    let Some((tree, _)) = parse_as_code(file_path, content, deadline) else {
        return Ok(None);
    };
    let root = tree.root_node();

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
    let result = pick_candidate(candidates, fuzzy, lines)?;
    check_budget(deadline)?;
    Ok(result)
}

/// `error` (a miss at or near a line) naming the zero-based lines the symbol
/// occurs on, `place` saying how it was read: the agent's next `line`. No line, no addition.
fn name_lines(error: Error, place: &str, lines: &[usize]) -> Error {
    const LISTED: usize = 8;
    if lines.is_empty() {
        return error;
    }
    let listed = lines
        .iter()
        .take(LISTED)
        .map(|line| (line + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let more = match lines.len().saturating_sub(LISTED) {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    Error::new(format!(
        "{error}; {place} it occurs on line(s) {listed}{more}"
    ))
}

/// An `occurrence` past the occurrences of `symbol_name` on `line_hint`.
fn order_hint_error(symbol_name: &str, line_hint: u32, count: usize) -> Error {
    Error::new(format!(
        "Line {line_hint} has {count} occurrence(s) of '{symbol_name}'; occurrence is 0-based, at most {}",
        count - 1
    ))
}

#[cfg(test)]
mod tests;
