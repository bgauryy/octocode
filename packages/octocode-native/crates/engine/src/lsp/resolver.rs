use crate::error::{Error, Result};
use crate::lsp::grammar::grammar_for_file;
use crate::lsp::types::{LspFuzzyPosition, LspResolvedSymbol};
use crate::signatures::extractor::AST_EXECUTION_TIMEOUT;
use std::time::Instant;

mod line_scan;
mod syntax;

use line_scan::{scan_near_line, scan_whole_file};
use syntax::{collect_symbol_candidates, pick_candidate};

const DEFAULT_RADIUS: i32 = 5;
const MAX_POSITION_SOURCE_BYTES: usize = super::MAX_LSP_SOURCE_BYTES as usize;

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
    if size > MAX_POSITION_SOURCE_BYTES {
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
    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
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
    resolve_position_before(
        file_path,
        content,
        fuzzy,
        Instant::now() + AST_EXECUTION_TIMEOUT,
    )
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
    if content.len() > MAX_POSITION_SOURCE_BYTES || symbol_name.is_empty() {
        return None;
    }
    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
    let spec = grammar_for_file(file_path)?;
    let tree = spec.parse_before(content, deadline)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
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
    resolve_position_from_lines(&lines, fuzzy, deadline)
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
    let result = pick_candidate(candidates, fuzzy, lines)?;
    check_budget(deadline)?;
    Ok(result)
}

/// An `orderHint` past the occurrences of `symbol_name` on `line_hint`.
fn order_hint_error(symbol_name: &str, line_hint: u32, count: usize) -> Error {
    Error::new(format!(
        "Line {line_hint} has {count} occurrence(s) of '{symbol_name}'; orderHint is 0-based, at most {}",
        count - 1
    ))
}

#[cfg(test)]
mod tests;
