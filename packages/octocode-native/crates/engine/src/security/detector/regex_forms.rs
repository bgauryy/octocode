//! The compiled forms of each secret pattern.
//!
//! A pattern's Unicode classes are most of the detector's memory: `[\\w-]{80,120}`
//! expands a Unicode class 120 times (6.6 MB for one pattern, against 19 KB in
//! ASCII). Yet text with no non-ASCII byte can only ever match ASCII, where the two
//! forms of `\\w`, `\\d` and `\\s` hold the same members. So each pattern compiles up
//! to twice: the ASCII form serves all-ASCII text, and any other text gets the full
//! form. Serving non-ASCII text from the ASCII form would leak secrets: a token with
//! one accented letter, or a key name or spacing outside ASCII, stops matching.
use super::super::patterns::PATTERNS;
use regex::Regex;
use regex_automata::hybrid::dfa::DFA;
use regex_automata::nfa::thompson::{self, NFA, WhichCaptures};
use regex_automata::{MatchKind, util::syntax};
use std::sync::{LazyLock, OnceLock};

type Cells = LazyLock<Vec<OnceLock<Regex>>>;

/// Lazily-compiled per-pattern Regex instances (compile on first use per index).
/// Sized from `PATTERNS.len()` — never a hardcoded count — so the cell table
/// can never be shorter than the pattern set it indexes.
static FULL_CELLS: Cells = LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());
static ASCII_CELLS: Cells =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());
static ANCHORED_FULL_CELLS: Cells =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());
static ANCHORED_ASCII_CELLS: Cells =
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

/// The ASCII Regex of pattern `idx`. It answers only for all-ASCII text.
pub(super) fn ascii_pattern_regex(idx: usize) -> &'static Regex {
    compiled(&ASCII_CELLS, idx, || {
        ascii_classes(&ascii_word_boundaries(PATTERNS[idx].regex))
    })
}

/// The cheapest Regex of pattern `idx` that answers exactly for `haystack`.
pub(super) fn pattern_regex_for(idx: usize, haystack: &str) -> &'static Regex {
    if haystack.is_ascii() {
        ascii_pattern_regex(idx)
    } else {
        pattern_regex(idx)
    }
}

/// The Regex of pattern `idx` that matches only at the start of its haystack
/// (`\A`), in the form that answers exactly for text like `ascii`'s. A search from
/// the one place a match can begin ends where that match fails, instead
/// of reading on to the haystack's end for a later start.
pub(super) fn anchored_pattern_regex(idx: usize, ascii: bool) -> &'static Regex {
    if ascii {
        compiled(&ANCHORED_ASCII_CELLS, idx, || {
            format!(
                r"\A(?:{})",
                ascii_classes(&ascii_word_boundaries(PATTERNS[idx].regex))
            )
        })
    } else {
        compiled(&ANCHORED_FULL_CELLS, idx, || {
            format!(r"\A(?:{})", ascii_word_boundaries(PATTERNS[idx].regex))
        })
    }
}

static ANCHORED_DFAS: LazyLock<Vec<OnceLock<DFA>>> =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());
static NFAS: LazyLock<Vec<OnceLock<NFA>>> =
    LazyLock::new(|| (0..PATTERNS.len()).map(|_| OnceLock::new()).collect());

/// The Thompson NFA of pattern `idx`, with its capture groups, built once, in
/// the form that answers like [`anchored_pattern_regex`]'s full form: CRLF
/// line ends, ASCII word boundaries. Its anchored start reads a text one byte
/// at a time, so a caller can tell where two searches that start apart meet
/// ([`anchored_dfa`], [`super::tail_captures::TailCaptures`]).
pub(super) fn pattern_nfa(idx: usize) -> &'static NFA {
    // Built-in pattern strings are constants validated by the pattern tests; a
    // build failure is a build-time bug, not a runtime condition.
    #[expect(
        clippy::expect_used,
        reason = "built-in patterns are valid by construction"
    )]
    NFAS[idx].get_or_init(|| {
        thompson::Compiler::new()
            .syntax(syntax::Config::new().crlf(true))
            .configure(thompson::Config::new().which_captures(WhichCaptures::All))
            .build(&ascii_word_boundaries(PATTERNS[idx].regex))
            .expect(PATTERNS[idx].name)
    })
}

/// The lazy DFA of [`pattern_nfa`], built once: it reads a text one byte at a
/// time and ignores the captures, so a caller can tell where two searches that
/// start apart meet in one state ([`super::tail_reach::Reach`]).
pub(super) fn anchored_dfa(idx: usize) -> &'static DFA {
    #[expect(
        clippy::expect_used,
        reason = "built-in patterns are valid by construction"
    )]
    ANCHORED_DFAS[idx].get_or_init(|| {
        DFA::builder()
            .configure(DFA::config().match_kind(MatchKind::LeftmostFirst))
            .build_from_nfa(pattern_nfa(idx).clone())
            .expect(PATTERNS[idx].name)
    })
}
