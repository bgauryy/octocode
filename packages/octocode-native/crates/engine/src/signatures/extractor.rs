//! Generic tree-sitter signature extractor.
//!
//! Algorithm:
//!   1. Start with every line marked KEEP.
//!   2. Parse the file with the supplied language grammar.
//!   3. Walk the AST; for each function/method *body* node, mark its
//!      interior rows (start+1 .. end-1) as DROP.
//!   4. Bodies of class-like containers are NOT dropped — only the bodies
//!      of their *member* functions (handled by step 3 recursively).
//!
//! The Rust QueryCursor evaluates built-in text predicates. Queries requiring
//! application-specific predicates are rejected so unsupported filters cannot
//! silently remove source lines.

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;
use tree_sitter::{Language, Query, QueryCursor, QueryCursorOptions, StreamingIterator, Tree};

mod outline;
mod parse;

use outline::{
    ELIDED, collapse_annotation_runs, collapse_docstrings, collapse_import_runs,
    collapse_signature, drop_closing_rows, import_runs,
};
pub(crate) use parse::{
    ParseFailure, parse_before, parse_ranges_before, parse_ranges_ending, parse_with_deadline,
    whole_file_deadline,
};

/// Index in `chain` (strict ancestors, root first) of the outermost
/// parenthesized expression wrapping the node at `index` (or `index`).
fn skip_parens(chain: &[tree_sitter::Node<'_>], mut index: usize) -> usize {
    while index > 0 && chain[index - 1].kind() == "parenthesized_expression" {
        index -= 1;
    }
    index
}

/// Whether `body` is the body of a function expression invoked immediately
/// by a top-level statement (or one nested in such a body): `(function(){…})()`, `(() => {…})()`,
/// `!function(){…}()`, or `(function(){…}).call(this)`.
///
/// Reads `body`'s ancestors from one root-down descent: a `Node::parent()`
/// climb repeats that descent for every hop.
fn is_top_level_iife_body(root: tree_sitter::Node<'_>, body: tree_sitter::Node<'_>) -> bool {
    let mut chain = super::nodes::ancestors(root, body);
    // `chain[i - 1]` is the parent of `chain[i]`; the body's parent is last.
    while let Some(function_index) = chain.len().checked_sub(1) {
        let function = chain[function_index];
        if !matches!(
            function.kind(),
            "function_expression" | "function" | "arrow_function"
        ) {
            return false;
        }
        let mut callee = skip_parens(&chain, function_index);
        if let Some(member) = callee.checked_sub(1)
            && chain[member].kind() == "member_expression"
            && chain[member]
                .child_by_field_name("object")
                .is_some_and(|object| object.id() == chain[callee].id())
        {
            callee = skip_parens(&chain, member);
        }
        let Some(call) = callee
            .checked_sub(1)
            .filter(|call| chain[*call].kind() == "call_expression")
        else {
            return false;
        };
        if chain[call]
            .child_by_field_name("function")
            .is_none_or(|function| function.id() != chain[callee].id())
        {
            return false;
        }
        let mut statement = skip_parens(&chain, call);
        while statement > 0 && chain[statement - 1].kind() == "unary_expression" {
            statement = skip_parens(&chain, statement - 1);
        }
        // At file level, or directly inside another such IIFE body (legacy bundles
        // nest wrappers: `(function(){ (function(){ function api(){} })(); })()`).
        let Some(scope) = statement
            .checked_sub(2)
            .filter(|_| chain[statement - 1].kind() == "expression_statement")
        else {
            return false;
        };
        match chain[scope].kind() {
            "program" => return true,
            "statement_block" => chain.truncate(scope),
            _ => return false,
        }
    }
    false
}

pub struct LangExtractConfig {
    pub language: Language,
    /// Tree-sitter S-expression query; captures named `@body` must be the nodes to drop.
    pub body_query: &'static str,
}

/// Compiled `Query` objects are static per language (`body_query` is a
/// `&'static str` fixed in `languages.rs`) and safe to share across threads
/// once built. Queries can be shared concurrently; parsing requires mutable
/// parser access, so [`parse_with_deadline`] keeps a parser per worker thread.
/// Caches one compiled `Query` per `(language, body_query)` pair instead of
/// recompiling on every `extract()` call.
type QueryCacheKey = (Language, &'static str);
type QueryCacheMap = HashMap<QueryCacheKey, Arc<Query>>;

static QUERY_CACHE: OnceLock<RwLock<QueryCacheMap>> = OnceLock::new();

fn cached_query(language: &Language, body_query: &'static str) -> Option<Arc<Query>> {
    let cache = QUERY_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let key = (language.clone(), body_query);

    if let Some(query) = cache
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return Some(Arc::clone(query));
    }

    let query = Arc::new(Query::new(language, body_query).ok()?);
    if (0..query.pattern_count()).any(|index| {
        !query.general_predicates(index).is_empty() || !query.property_predicates(index).is_empty()
    }) {
        return None;
    }
    // A concurrent caller may have compiled the same query first: keep
    // theirs so every caller shares one instance.
    let mut cache = cache
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Some(Arc::clone(cache.entry(key).or_insert(query)))
}

/// Test helper: `(1-based line number, trimmed text)` pairs.
#[cfg(test)]
pub(crate) fn extract(content: &str, cfg: &LangExtractConfig) -> Option<Vec<(usize, String)>> {
    extract_with_limits(content, cfg, whole_file_deadline(), 65_536, false)
}

/// The extracted lines for the rendered outline: a run of top-level imports becomes
/// one summary line (`3| import … (lines 3-40)`), since an outline is for
/// the file's own declarations and imports read better as a range. A
/// multi-line signature and a module or class docstring shrink to their
/// first line, marked [`ELIDED`]; the declaration head's line range reads
/// the rest.
pub fn extract_outline(content: &str, cfg: &LangExtractConfig) -> Option<Vec<(usize, String)>> {
    let deadline = whole_file_deadline();
    let tree = parse_before(content, &cfg.language, deadline)?;
    outline_from_tree(content, cfg, &tree, deadline)
}

/// [`extract_outline`] from `tree`, a parse of `content` in `cfg`'s grammar:
/// one parse serves the kept lines and the import runs (and, through
/// `extract_signatures_with_declarations`, the declarations too).
pub(crate) fn outline_from_tree(
    content: &str,
    cfg: &LangExtractConfig,
    tree: &Tree,
    deadline: Instant,
) -> Option<Vec<(usize, String)>> {
    let kept = kept_lines(content, cfg, tree, deadline, 65_536, true)?;
    Some(collapse_import_runs(kept, &import_runs(tree.root_node())))
}

/// [`kept_lines`] after its own parse (tests drive the deadline and the
/// match limit through it).
#[cfg(test)]
fn extract_with_limits(
    content: &str,
    cfg: &LangExtractConfig,
    deadline: Instant,
    match_limit: u32,
    outline: bool,
) -> Option<Vec<(usize, String)>> {
    let tree = parse_before(content, &cfg.language, deadline)?;
    kept_lines(content, cfg, &tree, deadline, match_limit, outline)
}

/// The lines of `content` (parsed as `tree`) its body query keeps; an
/// `outline` also collapses signatures, docstrings and annotation runs and
/// drops closing-delimiter rows.
fn kept_lines(
    content: &str,
    cfg: &LangExtractConfig,
    tree: &Tree,
    deadline: Instant,
    match_limit: u32,
    outline: bool,
) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();
    if n == 0 {
        return None;
    }

    let mut keep = vec![true; n];
    let mut elided = vec![false; n];

    // Compile (or reuse the cached compile of) the body query; if it fails
    // (bad query or grammar mismatch), `None` sends the caller to its heuristic.
    let query = cached_query(&cfg.language, cfg.body_query)?;
    let mut cursor = QueryCursor::new();
    cursor.set_match_limit(match_limit);
    let body_capture = query.capture_index_for_name("body");
    let mut progress = |_: &tree_sitter::QueryCursorState| {
        if Instant::now() >= deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let mut matches = cursor.matches_with_options(
        &query,
        tree.root_node(),
        content.as_bytes(),
        QueryCursorOptions::new().progress_callback(&mut progress),
    );
    while let Some(m) = matches.next() {
        if Instant::now() >= deadline {
            return None;
        }
        for capture in m.captures() {
            if Some(capture.index) != body_capture {
                continue;
            }
            let node = capture.node;
            // A top-level IIFE body is the module's real scope (UMD
            // wrappers, legacy bundles): outline inside it, don't drop it.
            if is_top_level_iife_body(tree.root_node(), node) {
                continue;
            }
            let start = node.start_position().row;
            let end = node.end_position().row;

            // Detect brace-style vs indent-style body.
            // Brace-style: the body node's FIRST BYTE is `{` (JS/TS/Go/Rust/C/Java etc.)
            // Indent-style: first byte is NOT `{` (for example, a Python block).
            let body_first_byte = content.as_bytes().get(node.start_byte()).copied();
            let brace_style = body_first_byte == Some(b'{');
            let mut signature_end = start;

            if brace_style {
                // Keep opening `{` line ONLY; drop interior AND closing `}`.
                // Function heads are shown without the trailing `}`.
                // Class closing `}` is preserved naturally
                // because class_body is never queried.
                let hi = end.min(n.saturating_sub(1));
                if start < hi {
                    keep[(start + 1)..=hi].fill(false);
                }
            } else {
                // Drop all lines of the body (indent style). A body that
                // shares the signature's row (`def f(): return 1`) must
                // not erase the signature line.
                let hi = end.min(n.saturating_sub(1));
                let start_col = node.start_position().column;
                let sig_shares_row = lines.get(start).is_some_and(|l| {
                    l.as_bytes()[..start_col.min(l.len())]
                        .iter()
                        .any(|b| !b.is_ascii_whitespace())
                });
                let lo = if sig_shares_row { start + 1 } else { start };
                if lo <= hi {
                    keep[lo..=hi].fill(false);
                }
                signature_end = lo.saturating_sub(1);
            }
            if outline && let Some(function) = node.parent() {
                collapse_signature(function, signature_end, &mut keep, &mut elided);
            }
        }
    }
    drop(matches);
    if cursor.did_exceed_match_limit() || Instant::now() >= deadline {
        return None;
    }
    if outline {
        collapse_docstrings(tree.root_node(), &lines, &mut keep, &mut elided);
        collapse_annotation_runs(tree.root_node(), &lines, &mut keep, &mut elided);
        drop_closing_rows(&lines, &mut keep);
    }

    let result: Vec<(usize, String)> = keep
        .iter()
        .enumerate()
        .filter(|&(_, &keep)| keep)
        .map(|(i, _)| {
            let mut text = lines[i].trim_end().to_string();
            if elided[i] {
                text.push_str(ELIDED);
            }
            (i + 1, text)
        })
        .collect();

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests;
