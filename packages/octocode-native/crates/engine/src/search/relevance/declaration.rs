//! Whether a matched line's first match is the name a declaration
//! introduces: the keyword, modifier and statement word sets, and the
//! lexical walk over a line's words.

/// Words that introduce a declared name.
const DECLARATION_KEYWORDS: &[&[u8]] = &[
    b"fn",
    b"def",
    b"class",
    b"struct",
    b"enum",
    b"trait",
    b"impl",
    b"interface",
    b"type",
    b"typedef",
    b"const",
    b"let",
    b"var",
    b"val",
    b"func",
    b"function",
    b"module",
    b"mod",
    b"namespace",
    b"union",
    b"record",
    b"object",
    b"macro_rules!",
    b"#define",
];

/// Whether `word` introduces a declared name (`fn`, `class`, `#define`): the
/// one keyword set both the relevance ranking of a line and a caller that
/// reads a search such as `fn merge` as a declaration share.
pub fn is_declaration_keyword(word: &str) -> bool {
    DECLARATION_KEYWORDS.contains(&word.as_bytes())
}

/// Words that may precede a declaration keyword, or its name (`let mut x`).
const DECLARATION_MODIFIERS: &[&[u8]] = &[
    b"pub",
    b"export",
    b"default",
    b"public",
    b"private",
    b"protected",
    b"internal",
    b"static",
    b"async",
    b"abstract",
    b"final",
    b"override",
    b"extern",
    b"unsafe",
    b"inline",
    b"virtual",
    b"open",
    b"sealed",
    b"data",
    b"declare",
    b"readonly",
    b"synchronized",
    b"native",
    b"transient",
    b"volatile",
    b"mut",
];

/// Whether the byte `first_match` falls on the name a declaration introduces:
/// `pub fn name`, `export default class Name`, `let mut name`, `#define NAME`,
/// a Go method `func (r *T) name`, or a C-family member led by modifiers and a
/// type (`public static <T> T name(`, `private final long name =`).
pub(super) fn declares_at(line: &[u8], indent: usize, first_match: usize) -> bool {
    let words = Words { line, at: indent };
    let mut lead = Lead::Start;
    // `<K, V>` type parameters and `(r *T)` receivers span several words.
    let mut group: Option<u8> = None;
    for (start, end) in words.take(10) {
        let word = &line[start..end];
        if let Some(close) = group {
            if word.ends_with(&[close]) {
                group = None;
            }
            continue;
        }
        match lead {
            Lead::Keyword { .. } if word == b"mut" => {}
            Lead::Keyword { func: true } if word.starts_with(b"(") => {
                group = (!word.ends_with(b")")).then_some(b')');
            }
            Lead::Keyword { .. } => return (start..end).contains(&first_match),
            Lead::Typed => return (start..start + ident_len(word)).contains(&first_match),
            Lead::Start | Lead::Modified => {
                // `pub(crate)` and `pub(super)` are the `pub` modifier.
                let bare = word.split(|byte| *byte == b'(').next().unwrap_or(word);
                if DECLARATION_KEYWORDS.contains(&word) {
                    lead = Lead::Keyword {
                        func: word == b"func",
                    };
                } else if DECLARATION_MODIFIERS.contains(&bare) || is_annotation(word) {
                    lead = Lead::Modified;
                } else if lead == Lead::Start {
                    return false;
                } else if word.starts_with(b"<") {
                    group = (!word.ends_with(b">")).then_some(b'>');
                } else if ident_len(word) == 0 || STATEMENT_WORDS.contains(&bare) {
                    return false;
                } else if names_here(line, start, word) {
                    return (start..start + ident_len(word)).contains(&first_match);
                } else {
                    lead = Lead::Typed;
                }
            }
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lead {
    /// No word yet.
    Start,
    /// Only modifiers and annotations so far: a type or a name follows.
    Modified,
    /// A declaration keyword: the next word is the name (after a Go
    /// method's receiver when the keyword is `func`).
    Keyword { func: bool },
    /// Modifiers and a type: the next word is the name.
    Typed,
}

/// Words that open a statement rather than name a type after modifiers
/// (`pub use x;`, `export default new X()`).
const STATEMENT_WORDS: &[&[u8]] = &[
    b"use",
    b"import",
    b"return",
    b"new",
    b"await",
    b"throw",
    b"yield",
    b"extends",
    b"implements",
    b"in",
    b"of",
    b"as",
    b"is",
];

/// `@Override`, `@Nullable`, `@Component(...)`.
fn is_annotation(word: &[u8]) -> bool {
    word.len() > 1 && word[0] == b'@' && (word[1].is_ascii_alphabetic() || word[1] == b'_')
}

/// Length of the identifier prefix of `word` (`name` in `name(`, `name;`).
fn ident_len(word: &[u8]) -> usize {
    if word.first().is_none_or(u8::is_ascii_digit) {
        return 0;
    }
    word.iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
        .count()
}

/// Whether the identifier opening `word` (at `start`) is itself the declared
/// name rather than a type: a parameter list, initializer, type annotation, or
/// terminator follows it (`name(`, `name =`, `name:`, `name;`, `name,`).
fn names_here(line: &[u8], start: usize, word: &[u8]) -> bool {
    let after = start + ident_len(word);
    let next = line[after..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|offset| after + offset);
    match next.map(|at| (line[at], line.get(at + 1).copied())) {
        Some((b'=', Some(b'=' | b'>'))) => false,
        Some((b'(' | b'=' | b':' | b';' | b',', _)) => true,
        _ => false,
    }
}

/// Whitespace-separated `(start, end)` word spans from `at`.
struct Words<'a> {
    line: &'a [u8],
    at: usize,
}

impl Iterator for Words<'_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let line = self.line;
        let start = self.at
            + line[self.at..]
                .iter()
                .take_while(|byte| byte.is_ascii_whitespace())
                .count();
        let end = start
            + line[start..]
                .iter()
                .take_while(|byte| !byte.is_ascii_whitespace())
                .count();
        self.at = end;
        (end > start).then_some((start, end))
    }
}
