//! Comment and literal regions from a real parse, one rule for every linked
//! grammar: a grammar names its own comment and literal nodes, so raw strings,
//! lifetimes, regexes and template literals are right by construction.

use tree_sitter::{Language, Tree};

use super::apply::WhitespaceOnly;
use super::grammars::{self, Spacing};
use super::layout::{Indent, Region, RegionKind};
use crate::signatures::extractor::{ParseFailure, parse_until_stopped};

/// Node kinds that hold comment text: every grammar's name for one has this
/// in it (`line_comment`, `block_comment`, `html_comment`).
const COMMENT_KIND: &str = "comment";

/// Node kinds whose whitespace is content: every grammar's names for them
/// have one of these in them. `template_string` and `template_literal`, not
/// `template`, which C++ uses for whole declarations.
const LITERAL_KINDS: &[&str] = &[
    "string",
    "template_literal",
    "regex",
    "char",
    "rune",
    "jsx_text",
    "preproc_arg",
];

/// What a grammar names a source's comments and literals with, and whether
/// its layout is syntax.
pub(super) struct Lexed {
    pub(super) regions: Vec<Region>,
    pub(super) indent: Indent,
    pub(super) spacing: Spacing,
    /// The grammar that read it.
    pub(super) grammar: grammars::Grammar,
}

/// The regions of `source` under the first grammar for `extension` that
/// reads it without an error: the extension's owner, then any grammar that
/// also parses it (a `.h` of C++ is not C). Else the first that reads most of
/// it: what it cannot read (macros a grammar does not expand) stays as it is.
pub(super) fn lex(
    source: &str,
    extension: &str,
    stop: &dyn Fn() -> bool,
) -> Result<Lexed, WhitespaceOnly> {
    let candidates = grammars::for_extension(extension);
    if candidates.is_empty() {
        return Err(WhitespaceOnly::NoGrammar);
    }
    let mut readable = None;
    for grammar in candidates {
        let tree = match parse_until_stopped(source, grammar.language, stop) {
            Ok(tree) => tree,
            Err(ParseFailure::Interrupted) => return Err(WhitespaceOnly::Interrupted),
            // Past the linear work bound, or a grammar the parser refuses:
            // the next grammar may read it, else it is whitespace only.
            Err(ParseFailure::Unbounded | ParseFailure::Language(_)) => continue,
        };
        let (mut regions, unread) = regions_of(&tree, source);
        if let Some(lexis) = grammar.lexis {
            regions =
                super::lexis::agree(source, regions, &super::lexis::spans(source, lexis), lexis);
        }
        let lexed = Lexed {
            regions,
            indent: indent_of(grammar.language),
            spacing: grammar.spacing,
            grammar,
        };
        if unread == 0 {
            return Ok(lexed);
        }
        if readable.is_none() && unread * 2 < source.len() {
            readable = Some(lexed);
        }
    }
    readable.ok_or(WhitespaceOnly::SyntaxErrors)
}

/// A grammar whose scanner emits indent tokens has significant layout.
fn indent_of(language: &Language) -> Indent {
    // Node kind ids are `u16`: a count past it has no further ids to ask.
    let kinds = u16::try_from(language.node_kind_count()).unwrap_or(u16::MAX);
    let scans_indents = (0..kinds).any(|id| {
        language
            .node_kind_for_id(id)
            .is_some_and(|kind| kind == "_indent")
    });
    if scans_indents {
        Indent::Keep
    } else {
        Indent::Strip
    }
}

fn kind_of(name: &str) -> Option<RegionKind> {
    if name.contains(COMMENT_KIND) {
        Some(RegionKind::Comment)
    } else if LITERAL_KINDS.iter().any(|kind| name.contains(kind)) {
        Some(RegionKind::Literal)
    } else {
        None
    }
}

/// Node walk that stops at each comment, literal or `ERROR` node: what is
/// inside one is its text, whatever nodes a grammar gives it, and what the
/// grammar could not read is kept as it is. Also returns the bytes it could
/// not read. `TreeCursor` is iterative, so a deep tree costs no call stack.
fn regions_of(tree: &Tree, source: &str) -> (Vec<Region>, usize) {
    let mut regions = Vec::new();
    let mut unread = 0;
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let (start, end) = (node.start_byte(), node.end_byte());
        let found = if node.is_error() {
            unread += end - start;
            Some(RegionKind::Literal)
        } else if let Some(kind) = node.is_named().then(|| kind_of(node.kind())).flatten() {
            Some(kind)
        } else {
            // A token with whitespace inside (C's `#  define`, a shebang line
            // ending in `\r`) is one token: its whitespace is part of it.
            (node.child_count() == 0 && source[start..end].bytes().any(|b| b.is_ascii_whitespace()))
                .then_some(RegionKind::Literal)
        };
        if let Some(kind) = found {
            // A shebang is a comment to its language, and read by the kernel.
            let read_by_a_tool = kind == RegionKind::Comment
                && ((start == 0 && source.starts_with("#!"))
                    || super::directive::is_directive(&source[start..end]));
            regions.push(Region {
                start,
                end,
                kind: if read_by_a_tool {
                    RegionKind::Literal
                } else {
                    kind
                },
            });
        } else if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                keep_cgo_preambles(source, &mut regions);
                return (regions, unread);
            }
        }
    }
}

/// The comments right above Go's `import "C"` are C code that cgo compiles.
fn keep_cgo_preambles(source: &str, regions: &mut [Region]) {
    let mut attached = false;
    let mut next_start = source.len();
    for region in regions.iter_mut().rev() {
        let after = &source[region.end..];
        let gap_is_blank = source[region.end..next_start].trim().is_empty();
        attached = region.kind == RegionKind::Comment
            && ((attached && gap_is_blank) || after.trim_start().starts_with("import \"C\""));
        if attached {
            region.kind = RegionKind::Literal;
        }
        next_start = region.start;
    }
}
