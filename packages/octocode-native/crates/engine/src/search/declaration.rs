//! Whether a matched line's first match is the name a declaration
//! introduces, which marks a row `declaration: true`: the keyword, modifier and statement word sets, and the
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

/// The name a declaration head introduces as its last word: `fn merge`,
/// `pub(crate) fn merge`, `export default class Merge`, `let mut merge`,
/// `@Override public String merge`, a Go method `func (r *T) merge`. One walk
/// ([`declares_at`]) both marks a matched line `declaration` and reads a
/// search such as `pub fn merge` as a declaration, so the two agree. A head
/// written up to its signature's opening (`fn merge(`, `fn merge<T>`)
/// names the identifier its last word starts with. `None` for any other
/// text, a lone word among them.
pub fn declared_name(text: &str) -> Option<&str> {
    let head = text.trim_ascii();
    let (_, word) = head.rsplit_once(|c: char| c.is_ascii_whitespace())?;
    if !declares_at(head.as_bytes(), 0, head.len() - word.len()) {
        return None;
    }
    let first = word.chars().next()?;
    let end = word
        .char_indices()
        .skip(1)
        .find(|(_, c)| !crate::text::is_ident_char(*c))
        .map_or(word.len(), |(at, _)| at);
    word.get(..end.max(first.len_utf8()))
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
pub(crate) fn declares_at(line: &[u8], indent: usize, first_match: usize) -> bool {
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

use crate::types::DECLARATION_RANK;

/// Class of one matched line: [`DECLARATION_RANK`] when its first match
/// (starting at byte `first_match`) is the name a declaration introduces
/// in code, `0` otherwise, including inside a comment or a string literal.
/// Only the row's `declaration` flag reads it; nothing orders by it.
pub(crate) fn line_rank(line: &[u8], first_match: usize) -> u32 {
    let first_match = first_match.min(line.len());
    let indent = line
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    if starts_comment(&line[indent..])
        || in_string_or_comment(line, indent, first_match)
        || !declares_at(line, indent, first_match)
    {
        0
    } else {
        DECLARATION_RANK
    }
}

fn starts_comment(text: &[u8]) -> bool {
    let next_is_blank = |at: usize| text.get(at).is_none_or(u8::is_ascii_whitespace);
    text.starts_with(b"//")
        || text.starts_with(b"/*")
        || text.starts_with(b"<!--")
        || (text.starts_with(b"*") && (next_is_blank(1) || text.get(1) == Some(&b'/')))
        || (text.starts_with(b"#") && (next_is_blank(1) || text.get(1) == Some(&b'#')))
        || (text.starts_with(b"--") && next_is_blank(2))
}

/// Whether byte `end` sits inside a `"`/`` ` `` string or after a `//` or
/// `/*` comment opener on this line. Single quotes are not tracked: they are
/// Rust lifetimes and char literals as often as strings.
fn in_string_or_comment(line: &[u8], start: usize, end: usize) -> bool {
    let mut quote = None;
    let mut at = start;
    while at < end {
        let byte = line[at];
        match quote {
            Some(_) if byte == b'\\' => at += 1,
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'`' => quote = Some(byte),
            None if byte == b'/' && matches!(line.get(at + 1), Some(b'/' | b'*')) => return true,
            None => {}
        }
        at += 1;
    }
    quote.is_some()
}

#[cfg(test)]
mod tests;
