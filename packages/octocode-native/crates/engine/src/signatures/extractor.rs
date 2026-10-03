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

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};
use tree_sitter::{
    Language, ParseOptions, Parser, Query, QueryCursor, QueryCursorOptions, StreamingIterator, Tree,
};

pub(crate) const AST_EXECUTION_TIMEOUT: Duration = Duration::from_secs(2);

thread_local! {
    /// The one worker-local tree-sitter parser. Repository scans reuse its
    /// allocations without retaining syntax trees or sharing mutable parser
    /// state; every tree-sitter parse in the engine goes through
    /// [`parse_with_deadline`].
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

/// Why [`parse_with_deadline`] produced no tree.
#[derive(Debug)]
pub(crate) enum ParseFailure {
    /// The grammar was rejected by the parser (ABI mismatch).
    Language(String),
    /// The deadline passed before or during the parse.
    Interrupted,
}

/// Parse `content` with `language` on the worker-local parser, cancelling at
/// `deadline`. The parser is reset before and after every call (success,
/// interruption, or error), so the next independent file never inherits
/// cancellation or incremental parse state.
pub(crate) fn parse_with_deadline(
    content: &str,
    language: &Language,
    deadline: Instant,
) -> Result<Tree, ParseFailure> {
    if Instant::now() >= deadline {
        return Err(ParseFailure::Interrupted);
    }
    PARSER.with_borrow_mut(|parser| {
        parser.reset();
        parser
            .set_language(language)
            .map_err(|err| ParseFailure::Language(err.to_string()))?;
        let bytes = content.as_bytes();
        let mut read = |offset: usize, _| bytes.get(offset..).unwrap_or(b"");
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
        );
        parser.reset();
        tree.filter(|_| Instant::now() < deadline)
            .ok_or(ParseFailure::Interrupted)
    })
}

/// Parse only `ranges` of `content` (tree-sitter included ranges): node
/// positions stay in `content` coordinates, so one `LineIndex` serves both
/// trees. Used to read Rust item-level macro bodies as items.
pub(crate) fn parse_ranges_before(
    content: &str,
    language: &Language,
    ranges: &[tree_sitter::Range],
    deadline: Instant,
) -> Option<Tree> {
    if Instant::now() >= deadline {
        return None;
    }
    PARSER.with_borrow_mut(|parser| {
        parser.reset();
        parser.set_language(language).ok()?;
        parser.set_included_ranges(ranges).ok()?;
        let bytes = content.as_bytes();
        let mut read = |offset: usize, _| bytes.get(offset..).unwrap_or(b"");
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
        );
        let _ = parser.set_included_ranges(&[]);
        parser.reset();
        tree.filter(|_| Instant::now() < deadline)
    })
}

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

/// [`parse_with_deadline`] for callers that treat every failure alike.
pub(crate) fn parse_before(content: &str, language: &Language, deadline: Instant) -> Option<Tree> {
    parse_with_deadline(content, language, deadline).ok()
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

    if let Ok(cache) = cache.read()
        && let Some(query) = cache.get(&key)
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
    if let Ok(mut cache) = cache.write() {
        return Some(Arc::clone(cache.entry(key).or_insert(query)));
    }
    Some(query)
}

/// Returns `(1-based line number, trimmed text)` pairs.
pub fn extract(content: &str, cfg: &LangExtractConfig) -> Option<Vec<(usize, String)>> {
    extract_with_limits(content, cfg, Instant::now() + AST_EXECUTION_TIMEOUT, 65_536)
}

/// [`extract`] for the rendered outline: a run of top-level imports becomes
/// one summary line (`3| import … (lines 3-40)`), since an outline is for
/// the file's own declarations and imports read better as a range.
pub fn extract_outline(content: &str, cfg: &LangExtractConfig) -> Option<Vec<(usize, String)>> {
    let deadline = Instant::now() + AST_EXECUTION_TIMEOUT;
    let kept = extract_with_limits(content, cfg, deadline, 65_536)?;
    let tree = parse_before(content, &cfg.language, deadline)?;
    Some(collapse_import_runs(kept, &import_runs(tree.root_node())))
}

/// Top-level import-like statements across the registry's grammars.
fn is_import_kind(kind: &str) -> bool {
    matches!(
        kind,
        "import_statement"
            | "import_from_statement"
            | "future_import_statement"
            | "import_declaration"
            | "import_header"
            | "use_declaration"
            | "extern_crate_declaration"
            | "preproc_include"
            | "using_directive"
            | "import"
    )
}

/// 0-based row spans of consecutive top-level imports; comments between
/// imports do not end a run.
fn import_runs(root: tree_sitter::Node<'_>) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut open = false;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();
        if is_import_kind(kind) {
            let (start, end) = (child.start_position().row, child.end_position().row);
            match runs.last_mut() {
                Some(run) if open => run.1 = end,
                _ => runs.push((start, end)),
            }
            open = true;
        } else if !kind.contains("comment") {
            open = false;
        }
    }
    runs
}

/// Replace each import run holding more than one kept line with one line:
/// the run's first word and its 1-based line range.
fn collapse_import_runs(
    kept: Vec<(usize, String)>,
    runs: &[(usize, usize)],
) -> Vec<(usize, String)> {
    let mut out = Vec::with_capacity(kept.len());
    let mut index = 0;
    while index < kept.len() {
        let line = kept[index].0;
        let Some(&(start, end)) = runs
            .iter()
            .find(|(start, end)| (start + 1..=end + 1).contains(&line))
        else {
            out.push(kept[index].clone());
            index += 1;
            continue;
        };
        let run_end = kept[index..]
            .iter()
            .take_while(|(line, _)| *line <= end + 1)
            .count();
        let members = &kept[index..index + run_end];
        let count = members
            .iter()
            .filter(|(_, text)| !text.trim().is_empty())
            .count();
        if count <= 1 {
            out.extend(members.iter().cloned());
        } else {
            let first_word = members
                .iter()
                .find_map(|(_, text)| text.split_whitespace().next())
                .unwrap_or("import");
            out.push((
                line,
                format!("{first_word} … (lines {}-{})", start + 1, end + 1),
            ));
        }
        index += run_end;
    }
    out
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
        .filter(|&(_, &keep)| keep)
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
        assert!(
            parse_before(
                "fn healthy() {}",
                &cfg.language,
                Instant::now() + Duration::from_secs(2)
            )
            .is_some(),
            "an interrupted parse must not poison worker-local parser scratch state"
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
    fn outline_collapses_top_level_import_runs_to_one_line() {
        let cfg = LangExtractConfig {
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            body_query: "(function_declaration body: (statement_block) @body)",
        };
        let source = "import a from 'a';\n// note\nimport {\n  b,\n} from 'b';\nimport c from 'c';\n\nexport function f() {\n  return a;\n}\nimport late from 'late';\n";
        let outline = extract_outline(source, &cfg).expect("outline");
        assert_eq!(
            outline,
            vec![
                (1, "import … (lines 1-6)".to_owned()),
                (7, String::new()),
                (8, "export function f() {".to_owned()),
                (11, "import late from 'late';".to_owned()),
            ]
        );
        // The boundary view keeps every line.
        assert_eq!(extract(source, &cfg).expect("lines").len(), 9);
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
            let lines = extract(
                source,
                &LangExtractConfig {
                    language,
                    body_query: query_src,
                },
            )
            .expect("outline");
            let outline = lines
                .into_iter()
                .map(|(_, text)| text)
                .collect::<Vec<_>>()
                .join("\n");
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
