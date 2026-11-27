//! The compiled forms of each secret pattern, and the handles a thread searches
//! them through.
//!
//! A pattern's Unicode classes are most of the detector's memory: `[\\w-]{80,120}`
//! expands a Unicode class 120 times (6.6 MB for one pattern, against 19 KB in
//! ASCII). The two forms of `\\w`, `\\d` and `\\s` hold the same ASCII members, and
//! disagree only on the characters outside ASCII that the Unicode classes hold:
//! letters, marks, digits, connector punctuation and spaces. So each pattern
//! compiles up to twice: the ASCII form serves any text that holds none of those
//! characters (all-ASCII text, and text whose other characters are symbols,
//! punctuation or emoji such as `…`, `→` or `✓`), and any other text gets the full
//! form ([`Classes::of`]). Serving such a text from the ASCII form would leak
//! secrets: a token with one accented letter, or a key name or spacing outside
//! ASCII, stops matching.
//!
//! A compiled regex keeps a search cache for each thread that searches it at once:
//! its lazy DFAs' tables, sized by the pattern's NFA states when the cache is made,
//! and the states they build as they search. A thread reaches each pattern through
//! its own handle ([`with_handles`]), a clone that shares the compiled program and
//! owns its cache, so the cache ends with the thread: the threads of one search
//! take theirs with them, instead of leaving one per thread in the process.
use super::super::patterns::PATTERNS;
use regex::Regex;
use regex_syntax::hir::{Class, HirKind};
use std::cell::Cell;
use std::sync::{LazyLock, OnceLock};

type Cells = LazyLock<Vec<OnceLock<Regex>>>;

/// Lazily-compiled per-pattern Regex instances (compile on first use per index).
/// Sized from `PATTERNS.len()` — never a hardcoded count — so the cell table
/// can never be shorter than the pattern set it indexes.
static FULL_CELLS: Cells = LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());
static ASCII_CELLS: Cells =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());

/// Rewrite Unicode `\\b` to ASCII `(?-u:\\b)`. A Unicode word boundary makes the
/// regex crate abandon its fast DFA on any non-ASCII haystack (12–20× slower on
/// multi-MB files). The ASCII boundary also fires next to non-ASCII letters, so
/// it only widens redaction around the ASCII token shapes these patterns match.
pub(super) fn ascii_word_boundaries(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('b') => out.push_str("(?-u:\\b)"),
            Some(escaped) => {
                out.push('\\');
                out.push(escaped);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// `pattern` with `\\w`, `\\d` and `\\s` spelled by their ASCII members, inside a
/// class or out. Negated forms stay: on ASCII text they already agree.
pub(super) fn ascii_classes(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 32);
    let mut chars = pattern.chars();
    let mut class_depth = 0usize;
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let members = match chars.next() {
                    Some('w') => "0-9A-Za-z_",
                    Some('d') => "0-9",
                    Some('s') => "\\t\\n\\x0B\\x0C\\r ",
                    Some(escaped) => {
                        out.push('\\');
                        out.push(escaped);
                        continue;
                    }
                    None => {
                        out.push('\\');
                        continue;
                    }
                };
                if class_depth > 0 {
                    out.push_str(members);
                } else {
                    out.push('[');
                    out.push_str(members);
                    out.push(']');
                }
            }
            '[' => {
                class_depth += 1;
                out.push(ch);
            }
            ']' => {
                class_depth = class_depth.saturating_sub(1);
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

fn compiled(cells: &'static Cells, idx: usize, source: impl FnOnce() -> String) -> &'static Regex {
    // Built-in pattern strings are constants validated by the pattern tests; a
    // compile failure is a build-time bug, not a runtime condition.
    #[expect(
        clippy::expect_used,
        reason = "built-in patterns are valid by construction"
    )]
    // CRLF mode: a multi-line `$` also ends a line before `\r\n`, so a value
    // in a CRLF file ends where it does in an LF one.
    cells[idx].get_or_init(|| {
        regex::RegexBuilder::new(&source())
            .crlf(true)
            .build()
            .expect(PATTERNS[idx].name)
    })
}

/// The full Regex of pattern `idx`, for any text.
pub(super) fn pattern_regex(idx: usize) -> &'static Regex {
    compiled(&FULL_CELLS, idx, || {
        ascii_word_boundaries(PATTERNS[idx].regex)
    })
}

/// The ASCII Regex of pattern `idx`. It answers exactly for a text that
/// [`Classes::of`] sends to it.
pub(super) fn ascii_pattern_regex(idx: usize) -> &'static Regex {
    compiled(&ASCII_CELLS, idx, || {
        ascii_classes(&ascii_word_boundaries(PATTERNS[idx].regex))
    })
}

/// The characters outside ASCII that the full form's `\\w`, `\\d` or `\\s` holds,
/// as sorted ranges: the only characters on which the two forms of a class
/// disagree. Parsed by the syntax crate the patterns compile with, so it is the
/// set they compile to. Were it ever unreadable, every character outside ASCII
/// counts, and such a text gets the full form, as it always may.
static UNICODE_CLASS_MEMBERS: LazyLock<Vec<(char, char)>> = LazyLock::new(|| {
    match regex_syntax::parse(r"[[\w\d\s]--[\x00-\x7F]]").map(regex_syntax::hir::Hir::into_kind) {
        Ok(HirKind::Class(Class::Unicode(class))) => class
            .ranges()
            .iter()
            .map(|range| (range.start(), range.end()))
            .collect(),
        _ => vec![('\u{80}', char::MAX)],
    }
});

/// Whether `ch` is a character on which the two forms of a class disagree.
fn is_unicode_class_member(ch: char) -> bool {
    UNICODE_CLASS_MEMBERS
        .binary_search_by(|&(start, end)| {
            if end < ch {
                std::cmp::Ordering::Less
            } else if start > ch {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// The compiled form of the patterns that answers exactly for one text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Classes {
    /// Every pattern's ASCII form.
    Ascii,
    /// Every pattern's full form.
    Unicode,
}

impl Classes {
    /// The cheapest form that answers exactly for `haystack`: the ASCII one
    /// unless `haystack` holds a character on which the forms disagree.
    pub(super) fn of(haystack: &str) -> Self {
        // An all-ASCII text is told a word at a time; any other is read once,
        // each character outside ASCII looked up.
        if haystack.is_ascii()
            || !haystack
                .chars()
                .any(|ch| !ch.is_ascii() && is_unicode_class_member(ch))
        {
            Self::Ascii
        } else {
            Self::Unicode
        }
    }

    /// Pattern `idx` compiled in this form, shared by every thread.
    pub(super) fn regex(self, idx: usize) -> &'static Regex {
        match self {
            Self::Ascii => ascii_pattern_regex(idx),
            Self::Unicode => pattern_regex(idx),
        }
    }
}

/// One thread's handles on the compiled patterns, by form and pattern index:
/// each made on the thread's first search of that pattern in that form.
#[derive(Default)]
pub(super) struct Handles {
    ascii: Vec<Option<Regex>>,
    unicode: Vec<Option<Regex>>,
}

impl Handles {
    /// No handle yet.
    const NONE: Self = Self {
        ascii: Vec::new(),
        unicode: Vec::new(),
    };

    /// This thread's handle on pattern `idx` compiled as `classes` says.
    pub(super) fn regex(&mut self, classes: Classes, idx: usize) -> &Regex {
        let handles = match classes {
            Classes::Ascii => &mut self.ascii,
            Classes::Unicode => &mut self.unicode,
        };
        if handles.is_empty() {
            handles.resize_with(PATTERNS.len(), || None);
        }
        handles[idx].get_or_insert_with(|| classes.regex(idx).clone())
    }
}

thread_local! {
    /// The handles of the thread: dropped, with the caches they own, when it ends.
    static HANDLES: Cell<Handles> = const { Cell::new(Handles::NONE) };
}

/// `search` run with this thread's [`Handles`]. They are taken out of the
/// thread's slot for the search and put back after it, so a search never
/// finds the slot in use; a thread whose slot is gone (it is ending) searches
/// through handles of its own for this search.
pub(super) fn with_handles<R>(search: impl FnOnce(&mut Handles) -> R) -> R {
    let mut handles = HANDLES.try_with(Cell::take).unwrap_or_default();
    let found = search(&mut handles);
    // A slot that is gone takes nothing back: the handles drop here.
    let _ = HANDLES.try_with(|slot| slot.set(handles));
    found
}
