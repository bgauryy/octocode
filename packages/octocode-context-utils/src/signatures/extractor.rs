//! Generic tree-sitter signature extractor.
//!
//! Algorithm:
//!   1. Start with every line marked KEEP.
//!   2. Parse the file with the supplied language grammar.
//!   3. Walk the AST; for each function/method *body* node, mark its
//!      interior rows (start+1 .. end-1) as DROP.
//!   4. Bodies of class-like containers are NOT dropped — only the bodies
//!      of their *member* functions (handled by step 3 recursively).

use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

pub struct LangExtractConfig {
    pub language: Language,
    /// Tree-sitter S-expression query; captures named `@body` must be the nodes to drop.
    pub body_query: &'static str,
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

    // Compile the body query; if it fails (bad query or grammar mismatch) fall
    // back gracefully to returning all non-blank lines (caller will fall back).
    if let Ok(query) = Query::new(&cfg.language, cfg.body_query) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());
        while let Some(m) = matches.next() {
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
