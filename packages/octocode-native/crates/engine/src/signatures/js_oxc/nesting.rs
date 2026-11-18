//! How deep a JS/TS file nests, measured before oxc reads it.
//!
//! oxc's recursive-descent parser, its semantic pass and this module's AST
//! walks recurse once per nesting level and have no depth guard. A native
//! stack overflow aborts the whole process and no `catch_unwind` can stop it,
//! so a file nested deeper than the deep-stack worker can hold never reaches
//! oxc: its callers read it with the tree-sitter grammar instead (whose parse
//! and walks are iterative) and disclose why ([`super::parse_failure`]).
//!
//! The depth is the tree-sitter syntax tree's, walked with a `TreeCursor`, so
//! every way to nest counts: brackets, operator chains, `else if`, unary
//! prefixes, arrows, JSX and type arguments alike. A one-pass bound on it
//! ([`scan`]) vouches for most files first, so they never pay for that tree.

use crate::deep_stack::{STACK_SIZE, job_cancelled};
use octocode_io::memo::Memo;

mod scan;

/// Stack one syntax-tree level may take across oxc's parse, its semantic
/// pass and the extraction walks, in the build that takes the most: the
/// deepest-recursing construct (`tests::nesting`), measured unoptimized and
/// rounded up.
const STACK_BYTES_PER_LEVEL: usize = 4 * 1024;

/// The share of [`STACK_SIZE`] a file's nesting may use: the rest is the
/// margin for the frames below the parse and the constructs that cost more
/// per level than the average.
const NESTING_SHARE_NUMERATOR: usize = 3;
const NESTING_SHARE_DENOMINATOR: usize = 4;

/// The deepest syntax tree oxc reads on a deep-stack worker.
pub(crate) const MAX_NESTING: usize =
    STACK_SIZE / NESTING_SHARE_DENOMINATOR * NESTING_SHARE_NUMERATOR / STACK_BYTES_PER_LEVEL;

/// A file oxc must not read: how deep it nests, and the line of the deepest
/// node (1-based). `depth` is `None` when its tree could not be measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooDeep {
    pub depth: Option<usize>,
    pub line: Option<u32>,
}

impl TooDeep {
    /// Why oxc did not read the file, for a fallback diagnostic.
    pub(crate) fn reason(self) -> String {
        match self.depth {
            Some(depth) => format!(
                "syntax nests {depth} levels deep, past the {MAX_NESTING} its parser's stack holds"
            ),
            None => format!(
                "syntax nesting could not be measured against the {MAX_NESTING} levels its parser's stack holds"
            ),
        }
    }
}

/// `Some` when `content`, read with the tree-sitter grammar of `ext`, nests
/// deeper than [`MAX_NESTING`]. Every level spans at least one byte, so a
/// file no longer than the limit is never parsed for this, nor one whose
/// one-pass bound stays within it.
pub(super) fn too_deep(content: &str, ext: &str) -> Option<TooDeep> {
    if content.len() <= MAX_NESTING || pass_bound(content, ext).is_some() {
        return None;
    }
    let Some((depth, row)) = measured_depth(content, ext) else {
        return Some(TooDeep {
            depth: None,
            line: None,
        });
    };
    (depth > MAX_NESTING).then(|| TooDeep {
        depth: Some(depth),
        line: Some(crate::num::saturating_u32(row) + 1),
    })
}

/// [`scan::nesting_bound`] of `content` in the dialect of `ext`: JavaScript
/// and TSX read JSX (Flow is read as TSX).
pub(super) fn pass_bound(content: &str, ext: &str) -> Option<usize> {
    let jsx = ext == "tsx"
        || crate::text::JS_EXTENSIONS.contains(&ext)
        || crate::text::file_extension::is_flow_source(content, ext);
    scan::nesting_bound(content, jsx, MAX_NESTING)
}

/// What [`syntax_depth`] measured of a text, by its digest and dialect: the
/// depth depends on them alone, so the analyses that read one text (facts,
/// declarations, reference sites; every page and repeat) parse it for this
/// once while the process budget keeps the answer.
static MEASURED: Memo<([u8; 32], String), (usize, usize)> = Memo::new(&crate::memory::MEMORY);

/// [`syntax_depth`], kept in [`MEASURED`]; a parse that stopped or found no
/// grammar measured nothing and is not kept.
fn measured_depth(content: &str, ext: &str) -> Option<(usize, usize)> {
    let key = (
        crate::digest::sha256_digest(content.as_bytes()),
        ext.to_owned(),
    );
    if let Some(kept) = MEASURED.get(&key) {
        return Some(*kept.value);
    }
    #[cfg(test)]
    measures::note(key.0);
    let measured = syntax_depth(content, ext)?;
    let weight = std::mem::size_of_val(&key) + key.1.len() + std::mem::size_of_val(&measured);
    MEASURED.insert(key, measured, weight, None);
    Some(measured)
}

/// The depth of `content`'s deepest syntax node read with the tree-sitter
/// grammar of `ext`, and its 0-based row. `None` when no grammar reads `ext`
/// or the parse was stopped.
pub(super) fn syntax_depth(content: &str, ext: &str) -> Option<(usize, usize)> {
    syntax_tree(content, ext).map(|tree| deepest(&tree))
}

fn syntax_tree(content: &str, ext: &str) -> Option<tree_sitter::Tree> {
    let grammar = crate::text::file_extension::grammar_extension(content, ext);
    let entry = crate::signatures::languages::find_entry(grammar)?;
    crate::signatures::extractor::parse_until_stopped(content, &entry.language, &job_cancelled).ok()
}

/// The depth of `tree`'s deepest node (the root is level 1) and its 0-based
/// row, walked iteratively. A region tree-sitter could not parse is kept as a
/// flat `ERROR` node, where oxc may still recurse once per token (`((((` with
/// no operand, `!!!!` at the end of the file): it counts as deep as its depth
/// plus everything under it.
fn deepest(tree: &tree_sitter::Tree) -> (usize, usize) {
    let mut cursor = tree.walk();
    let (mut depth, mut max, mut row) = (1, 1, cursor.node().start_position().row);
    loop {
        let node = cursor.node();
        let error = node.is_error();
        let reach = if error {
            depth + node.descendant_count()
        } else {
            depth
        };
        if reach > max {
            max = reach;
            row = node.start_position().row;
        }
        if !error && cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return (max, row);
            }
            depth -= 1;
        }
    }
}

/// Test-only count of the nesting parses ([`measured_depth`] misses) of each
/// text, by its digest, on any worker.
#[cfg(test)]
pub(super) mod measures;
