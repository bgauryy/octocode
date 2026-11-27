//! The grammars the compact view reads, one row each: every registry grammar,
//! plus the ones only the compact view uses so far (no outline or search is
//! built on them). A row carries what differs between grammars as data.

use std::sync::LazyLock;

use tree_sitter::Language;

use crate::signatures::languages;

/// Where a space separates nothing, in a grammar's own tokens: after one of
/// `after` or before one of `before`. Each listed byte is a token by itself
/// that joins no neighbor into another token in that grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Spacing {
    pub(super) after: &'static [u8],
    pub(super) before: &'static [u8],
}

/// Separators of the C, JS, Python, Go, Java, Scala, assembly and CSS
/// families. `[`, `]` stay out (C++ reads `[[` as an attribute), and so does
/// the space before `(` (`#define F (a)` is not `#define F(a)`) and around
/// `:` (`.a :hover` is not `.a:hover`). The token test in
/// `differential_tests` proves it per grammar.
pub(super) const SEPARATORS: Spacing = Spacing {
    after: b",;({",
    before: b",;)}",
};

/// Where a language's spec puts its comments and strings (`lexis`): what
/// compaction may remove and what stays byte for byte, whatever the grammar's
/// parse says, and what `verify` proves a view against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Lexis {
    /// Code points the language consumes as whitespace outside strings.
    pub(super) whitespace: &'static [u8],
    /// Opener and closer of a block comment.
    pub(super) block_comment: (&'static str, &'static str),
    /// Opener of a comment that runs to the end of its line, if any.
    pub(super) line_comment: Option<&'static str>,
    pub(super) quotes: &'static [u8],
    /// Takes the byte after it, a line break too, in a string or in code.
    pub(super) escape: u8,
    /// Calls, ASCII case aside, whose unquoted argument is one token in which
    /// no comment starts (`url(//cdn/a.png)`).
    pub(super) raw_calls: &'static [&'static str],
}

/// CSS Syntax 3, section 4: `/* */` is its only comment (a `//` line is text
/// of a rule), and a backslash-newline in a string is part of the string.
pub(super) const CSS_LEXIS: Lexis = Lexis {
    whitespace: b" \t\r\n\x0c",
    block_comment: ("/*", "*/"),
    line_comment: None,
    quotes: b"\"'",
    escape: b'\\',
    raw_calls: &["url("],
};

/// SCSS: CSS's, and `//` starts a silent comment.
pub(super) const SCSS_LEXIS: Lexis = Lexis {
    line_comment: Some("//"),
    ..CSS_LEXIS
};

#[derive(Clone, Copy)]
pub(super) struct Grammar {
    pub(super) language: &'static Language,
    pub(super) spacing: Spacing,
    /// The spec's comments and strings, where the row has them.
    pub(super) lexis: Option<&'static Lexis>,
}

struct CompactOnly {
    extensions: &'static [&'static str],
    language: &'static LazyLock<Language>,
    spacing: Spacing,
    lexis: Option<&'static Lexis>,
}

static CSS: LazyLock<Language> = LazyLock::new(|| tree_sitter_css::LANGUAGE.into());
static SCSS: LazyLock<Language> = LazyLock::new(tree_sitter_scss::language);

const COMPACT_ONLY: &[CompactOnly] = &[
    CompactOnly {
        extensions: &["css"],
        language: &CSS,
        spacing: SEPARATORS,
        lexis: Some(&CSS_LEXIS),
    },
    CompactOnly {
        extensions: &["scss"],
        language: &SCSS,
        spacing: SEPARATORS,
        lexis: Some(&SCSS_LEXIS),
    },
];

/// The grammars that may read a file with `extension`, in order: the
/// registry's owner, then any registry grammar that also parses it (a `.h` of
/// C++ is not C), else a compact-only row.
pub(super) fn for_extension(extension: &str) -> Vec<Grammar> {
    let registry = |entry: &'static languages::LanguageEntry| Grammar {
        language: &entry.language,
        spacing: SEPARATORS,
        lexis: None,
    };
    if let Some(owner) = languages::find_entry(extension) {
        return std::iter::once(owner)
            .chain(languages::also_parsing(extension))
            .map(registry)
            .collect();
    }
    COMPACT_ONLY
        .iter()
        .filter(|row| row.extensions.contains(&extension))
        .map(|row| Grammar {
            language: LazyLock::force(row.language),
            spacing: row.spacing,
            lexis: row.lexis,
        })
        .collect()
}
