use crate::error::{Error, Result};
use crate::lsp::grammar::grammar_for_file;
use crate::lsp::types::{LspFuzzyPosition, LspResolvedSymbol};
use crate::signatures::extractor::whole_file_deadline;
use std::time::Instant;

mod line_scan;
mod syntax;

use line_scan::{scan_near_line, scan_whole_file};
use syntax::{collect_symbol_candidates, pick_candidate};

const DEFAULT_RADIUS: usize = 5;

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
    if exceeds_source_cap(content.len()) || symbol_name.is_empty() {
        return None;
    }
    let deadline = whole_file_deadline();
    let tree = parse_as_code(file_path, content, deadline)?;
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
    Some(lines)
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
    // A miss near the line names where the symbol occurs as code, when the
    // grammar can tell: the agent's next `line`.
    resolve_position_from_lines(&lines, fuzzy, deadline)
        .map_err(|error| name_code_occurrences(error, file_path, content, &fuzzy.symbol_name))
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
fn parse_as_code(file_path: &str, content: &str, deadline: Instant) -> Option<tree_sitter::Tree> {
    let owner = grammar_for_file(file_path)?;
    let tree = owner.parse_before(content, deadline)?;
    if !tree.root_node().has_error() {
        return Some(tree);
    }
    let sibling = crate::lsp::grammar::sibling_grammar_for_file(file_path)?;
    sibling
        .parse_before(content, deadline)
        .filter(|tree| !tree.root_node().has_error())
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
    let Some(tree) = parse_as_code(file_path, content, deadline) else {
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

/// `error` (a miss at or near a line) naming the lines where the symbol
/// occurs as code, when the grammar can tell: the agent's next `line`.
fn name_code_occurrences(error: Error, file_path: &str, content: &str, symbol_name: &str) -> Error {
    const LISTED: usize = 8;
    match code_occurrence_lines(file_path, content, symbol_name) {
        Some(lines) if !lines.is_empty() => {
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
                "{error}; as code it occurs on line(s) {listed}{more}"
            ))
        }
        _ => error,
    }
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
