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
use std::time::{Duration, Instant};
use tree_sitter::{
    Language, ParseOptions, Parser, Query, QueryCursor, QueryCursorOptions, StreamingIterator, Tree,
};

pub(crate) const AST_EXECUTION_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn parse_before(content: &str, language: &Language, deadline: Instant) -> Option<Tree> {
    if Instant::now() >= deadline {
        return None;
    }
    let mut parser = Parser::new();
    parser.set_language(language).ok()?;
    let bytes = content.as_bytes();
    let mut read = |offset: usize, _| &bytes[offset..];
    let mut progress = |_: &tree_sitter::ParseState| {
        if Instant::now() >= deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let tree = parser.parse_with_options(
        &mut read,
        None,
        Some(ParseOptions::new().progress_callback(&mut progress)),
    )?;
    (Instant::now() < deadline).then_some(tree)
}

pub struct LangExtractConfig {
    pub language: Language,
    /// Tree-sitter S-expression query; captures named `@body` must be the nodes to drop.
    pub body_query: &'static str,
}

/// Compiled `Query` objects are static per language (`body_query` is a
/// `&'static str` fixed in `languages.rs`) and safe to share across threads
/// once built. Queries can be shared concurrently; parsing requires mutable
/// parser access, so `structural/octo.rs` keeps a parser per worker thread.
/// Caches one compiled `Query` per
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
    if (0..query.pattern_count()).any(|index| {
        !query.general_predicates(index).is_empty() || !query.property_predicates(index).is_empty()
    }) {
        return None;
    }
    if let Ok(mut cache) = cache.write() {
        cache.insert(key, Arc::clone(&query));
    }
    Some(query)
}

/// Returns `(1-based line number, trimmed text)` pairs.
pub fn extract(content: &str, cfg: &LangExtractConfig) -> Option<Vec<(usize, String)>> {
    extract_with_limits(content, cfg, Instant::now() + AST_EXECUTION_TIMEOUT, 65_536)
}

fn extract_with_limits(
    content: &str,
    cfg: &LangExtractConfig,
    deadline: Instant,
    match_limit: u32,
) -> Option<Vec<(usize, String)>> {
    let lines: Vec<&str> = content.lines().collect();
    let n = lines.len();
    if n == 0 {
        return None;
    }

    let mut keep = vec![true; n];

    let tree = parse_before(content, &cfg.language, deadline)?;

    // Compile (or reuse the cached compile of) the body query; if it fails
    // (bad query or grammar mismatch) fall back gracefully to returning all
    // non-blank lines (caller will fall back).
    if let Some(query) = cached_query(&cfg.language, cfg.body_query) {
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
        drop(matches);
        if cursor.did_exceed_match_limit() || Instant::now() >= deadline {
            return None;
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

    #[test]
    fn expired_signature_deadline_does_not_return_partial_boundaries() {
        let cfg = LangExtractConfig {
            language: tree_sitter_rust::LANGUAGE.into(),
            body_query: "(function_item body: (block) @body)",
        };
        assert!(
            extract_with_limits("fn f() {\n work();\n}\n", &cfg, Instant::now(), 65_536).is_none()
        );
    }

    #[test]
    fn exhausted_signature_query_does_not_return_partial_boundaries() {
        let cfg = LangExtractConfig {
            language: tree_sitter_rust::LANGUAGE.into(),
            body_query: "(block (expression_statement)* @body (expression_statement) @body)",
        };
        let source = "fn f() {\n one();\n two();\n three();\n four();\n}\n";
        assert!(
            extract_with_limits(source, &cfg, Instant::now() + Duration::from_secs(2), 1).is_none()
        );
        assert!(extract(source, &cfg).is_some());
    }

    #[test]
    fn helper_captures_do_not_remove_signature_lines() {
        let source = "def keep():\n    work()\n";
        let cfg = LangExtractConfig {
            language: tree_sitter_python::LANGUAGE.into(),
            body_query: "(function_definition body: (block) @body) @_function",
        };
        let outline = extract(source, &cfg).expect("outline retains the signature");
        assert_eq!(outline, vec![(1, "def keep():".to_owned())]);
    }

    #[test]
    fn rust_query_cursor_filters_builtin_text_predicates() {
        let source = "def strip():\n    removed = 1\n\ndef keep():\n    preserved = 2\n";
        for query_src in [
            "((function_definition name: (identifier) @_name body: (block) @body) (#eq? @_name \"strip\"))",
            "((function_definition name: (identifier) @_name body: (block) @body) (#any-of? @_name \"strip\" \"other\"))",
            "((function_definition name: (identifier) @_name body: (block) @body) (#match? @_name \"^strip$\"))",
        ] {
            let language = tree_sitter_python::LANGUAGE.into();
            let query = cached_query(&language, query_src).expect("valid built-in predicate");
            assert!(query.general_predicates(0).is_empty());
            let lines = extract(source, &LangExtractConfig { language, body_query: query_src })
                .expect("outline");
            let outline = lines.into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n");
            assert!(outline.contains("def strip"));
            assert!(!outline.contains("removed"), "{query_src}: {outline}");
            assert!(outline.contains("preserved"), "{query_src}: {outline}");
        }
    }

    #[test]
    fn unsupported_predicates_preserve_source_by_rejecting_the_query() {
        let language = tree_sitter_python::LANGUAGE.into();
        for query_src in [
            "((function_definition body: (block) @body) (#unsupported? @body))",
            "((function_definition body: (block) @body) (#is? local))",
        ] {
            assert!(cached_query(&language, query_src).is_none());
        }
    }

    #[test]
    fn cached_query_reuses_the_same_compiled_query_across_calls() {
        // Repeated calls for the same (language, body_query) must return the
        // same compiled Query (Arc::ptr_eq), not recompile it — the fix for
        // `extractor.rs` recompiling a static per-language query on every
        // file scanned.
        let lang: Language = tree_sitter_python::LANGUAGE.into();
        let query_src = r#"(function_definition body: (block) @body)"#;

        let first = cached_query(&lang, query_src).expect("query should compile");
        let second = cached_query(&lang, query_src).expect("query should compile");

        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "expected the second call to reuse the cached Query, got a distinct instance"
        );
    }

    #[test]
    fn cached_query_returns_none_for_an_invalid_query_without_poisoning_the_cache() {
        let lang: Language = tree_sitter_python::LANGUAGE.into();
        // Malformed query text — not a valid tree-sitter S-expression.
        let bad_query = "(this is not valid";
        assert!(cached_query(&lang, bad_query).is_none());

        // The cache must still work for a valid query afterward.
        let good_query = r#"(function_definition body: (block) @body)"#;
        assert!(cached_query(&lang, good_query).is_some());
    }
}
