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
//! Predicate evaluation
//! --------------------
//! `cursor.matches()` evaluates only *structural* constraints (node types,
//! fields, anchors). Text predicates — `#any-of?`, `#match?`, `#eq?` — are
//! returned from `query.general_predicates()` but are **not** automatically
//! applied. Without explicit evaluation the Elixir `#any-of?` predicate that
//! restricts `(call …)` to `def`/`defp`/`defmacro`/`defmacrop` would be
//! silently ignored, causing `defmodule` bodies to be stripped too.
//!
//! `predicates_satisfied` handles the subset of predicates used by the
//! built-in body queries. Unknown predicates default to *pass* so that
//! adding new predicates to body queries does not silently break extraction.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use tree_sitter::{Language, Parser, Query, QueryCursor, QueryPredicateArg, StreamingIterator};

pub struct LangExtractConfig {
    pub language: Language,
    /// Tree-sitter S-expression query; captures named `@body` must be the nodes to drop.
    pub body_query: &'static str,
}

/// Evaluate the text predicates attached to pattern `m.pattern_index` in `query`.
/// Returns `false` if any predicate is unsatisfied; `true` if all pass (or unknown).
/// Only `#any-of?`, `#eq?`, and `#match?` are implemented; everything else passes.
fn predicates_satisfied(
    query: &Query,
    m: &tree_sitter::QueryMatch<'_, '_>,
    content: &[u8],
) -> bool {
    for predicate in query.general_predicates(m.pattern_index) {
        // Strip leading `#` so both `any-of?` and `#any-of?` spellings work.
        let op = predicate.operator.trim_start_matches('#');
        match op {
            "any-of?" => {
                // Syntax: (#any-of? @capture "val1" "val2" …)
                let Some(QueryPredicateArg::Capture(cap_idx)) = predicate.args.first() else {
                    return false;
                };
                let cap_text = m
                    .captures
                    .iter()
                    .find(|c| c.index == *cap_idx)
                    .and_then(|c| c.node.utf8_text(content).ok())
                    .unwrap_or("");
                let found = predicate.args[1..].iter().any(
                    |arg| matches!(arg, QueryPredicateArg::String(s) if s.as_ref() == cap_text),
                );
                if !found {
                    return false;
                }
            }
            "eq?" => {
                // Syntax: (#eq? @capture "literal")  or  (#eq? @cap1 @cap2)
                if predicate.args.len() == 2 {
                    let lhs = capture_text(m, &predicate.args[0], content);
                    let rhs = capture_text(m, &predicate.args[1], content);
                    if lhs != rhs {
                        return false;
                    }
                }
            }
            "match?" => {
                // Syntax: (#match? @capture "regex")
                if let (
                    Some(QueryPredicateArg::Capture(cap_idx)),
                    Some(QueryPredicateArg::String(pattern)),
                ) = (predicate.args.first(), predicate.args.get(1))
                {
                    let cap_text = m
                        .captures
                        .iter()
                        .find(|c| c.index == *cap_idx)
                        .and_then(|c| c.node.utf8_text(content).ok())
                        .unwrap_or("");
                    // Use the `regex` crate already linked in the engine.
                    if let Ok(re) = regex::Regex::new(pattern) {
                        if !re.is_match(cap_text) {
                            return false;
                        }
                    }
                }
            }
            _ => {} // Unknown predicate: pass through (do not reject the match)
        }
    }
    true
}

/// Resolve a `QueryPredicateArg` to its text — either the literal string value
/// or the UTF-8 text of the matching capture node.
fn capture_text<'a>(
    m: &'a tree_sitter::QueryMatch<'_, '_>,
    arg: &'a QueryPredicateArg,
    content: &'a [u8],
) -> &'a str {
    match arg {
        QueryPredicateArg::String(s) => s.as_ref(),
        QueryPredicateArg::Capture(idx) => m
            .captures
            .iter()
            .find(|c| c.index == *idx)
            .and_then(|c| c.node.utf8_text(content).ok())
            .unwrap_or(""),
    }
}

/// Compiled `Query` objects are static per language (`body_query` is a
/// `&'static str` fixed in `languages.rs`) and safe to share across threads
/// once built — `tree_sitter::Query` is `Send + Sync`, unlike `Parser`, which
/// is why `structural/octo.rs` uses `thread_local!` for the parser
/// specifically rather than caching it here. Caches one compiled `Query` per
/// `(language, body_query)` pair instead of recompiling on every `extract()`
/// call — previously once per file scanned, mirroring the reuse pattern
/// `structural/files.rs` already uses for its matcher compilation.
type QueryCacheKey = (Language, &'static str);
type QueryCacheMap = HashMap<QueryCacheKey, Arc<Query>>;

static QUERY_CACHE: OnceLock<RwLock<QueryCacheMap>> = OnceLock::new();

fn cached_query(language: &Language, body_query: &'static str) -> Option<Arc<Query>> {
    let cache = QUERY_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    let key = (language.clone(), body_query);

    if let Ok(cache) = cache.read() {
        if let Some(query) = cache.get(&key) {
            return Some(Arc::clone(query));
        }
    }

    let query = Arc::new(Query::new(language, body_query).ok()?);
    if let Ok(mut cache) = cache.write() {
        cache.insert(key, Arc::clone(&query));
    }
    Some(query)
}

/// Returns `(1-based line number, trimmed text)` pairs.
pub fn extract(content: &str, cfg: &LangExtractConfig) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();
    if n == 0 {
        return None;
    }

    let mut keep = vec![true; n];

    let mut parser = Parser::new();
    parser.set_language(&cfg.language).ok()?;
    let tree = parser.parse(content.as_bytes(), None)?;

    // Compile (or reuse the cached compile of) the body query; if it fails
    // (bad query or grammar mismatch) fall back gracefully to returning all
    // non-blank lines (caller will fall back).
    if let Some(query) = cached_query(&cfg.language, cfg.body_query) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());
        while let Some(m) = matches.next() {
            // Skip matches that fail text predicates (#any-of?, #match?, #eq?).
            // cursor.matches() only checks structural constraints; predicates
            // must be evaluated explicitly.
            if !predicates_satisfied(&query, m, content.as_bytes()) {
                continue;
            }
            for capture in m.captures {
                let node = capture.node;
                let start = node.start_position().row;
                let end = node.end_position().row;

                // Detect brace-style vs indent-style body.
                // Brace-style: the body node's FIRST BYTE is `{` (JS/TS/Go/Rust/C/Java etc.)
                // Indent-style: first byte is NOT `{` (Python block, Ruby body_statement, etc.)
                let body_first_byte = content.as_bytes().get(node.start_byte()).copied();
                let brace_style = body_first_byte == Some(b'{');

                if brace_style {
                    // Keep opening `{` line ONLY; drop interior AND closing `}`.
                    // This matches TS behaviour: function heads are shown without
                    // the trailing `}`.  Class closing `}` is preserved naturally
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
                }
            }
        }
    } else {
        // Query failed → fall back to heuristic (signal with None)
        return None;
    }

    let result: Vec<(usize, String)> = keep
        .iter()
        .enumerate()
        .filter(|(_, &k)| k)
        .map(|(i, _)| (i + 1, lines[i].trim_end().to_string()))
        .collect();

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "tree-sitter-extended")]
    #[test]
    fn cached_query_reuses_the_same_compiled_query_across_calls() {
        // Repeated calls for the same (language, body_query) must return the
        // same compiled Query (Arc::ptr_eq), not recompile it — the fix for
        // `extractor.rs` recompiling a static per-language query on every
        // file scanned.
        let lang: Language = tree_sitter_elixir::LANGUAGE.into();
        let query_src = r#"(call (arguments (call)) (do_block) @body)"#;

        let first = cached_query(&lang, query_src).expect("query should compile");
        let second = cached_query(&lang, query_src).expect("query should compile");

        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "expected the second call to reuse the cached Query, got a distinct instance"
        );
    }

    #[cfg(feature = "tree-sitter-extended")]
    #[test]
    fn cached_query_returns_none_for_an_invalid_query_without_poisoning_the_cache() {
        let lang: Language = tree_sitter_elixir::LANGUAGE.into();
        // Malformed query text — not a valid tree-sitter S-expression.
        let bad_query = "(this is not valid";
        assert!(cached_query(&lang, bad_query).is_none());

        // The cache must still work for a valid query afterward.
        let good_query = r#"(call (arguments (call)) (do_block) @body)"#;
        assert!(cached_query(&lang, good_query).is_some());
    }

    #[cfg(feature = "tree-sitter-extended")]
    #[test]
    fn elixir_structural_query_filters_defmodule() {
        let lang: Language = tree_sitter_elixir::LANGUAGE.into();
        // Structural approach: def/defp/defmacro have (arguments (call)) — function
        // signature as arg. defmodule/defprotocol/defimpl have (arguments (alias)).
        let query_src = r#"
  (call
    (arguments
      (call))
    (do_block) @body)
"#;
        let src = "defmodule M do\n  def foo(a) do\n    a + 1\n  end\n  defp bar(b) do\n    b * 2\n  end\nend\n";
        let cfg = LangExtractConfig {
            language: lang,
            body_query: query_src,
        };
        let result = extract(src, &cfg);
        // Should produce: defmodule M do, def foo(a) do, defp bar(b) do, end(defmodule)
        // NOT just 2 lines (all lines stripped by defmodule body capture)
        let lines = result.expect("should extract");
        assert!(
            lines.len() > 2,
            "got only {} lines — defmodule body is still being stripped",
            lines.len()
        );
        // defmodule line must be kept
        assert!(
            lines.iter().any(|(_, t)| t.contains("defmodule")),
            "defmodule missing"
        );
        // def foo must be kept
        assert!(
            lines.iter().any(|(_, t)| t.contains("def foo")),
            "def foo missing"
        );
        // defp bar must be kept
        assert!(
            lines.iter().any(|(_, t)| t.contains("defp bar")),
            "defp bar missing"
        );
    }
}
