//! How far one match of a regex pattern can reach, read from the pattern's
//! syntax tree (the parse the `regex` crate compiles from), so a scan that
//! cuts a source into windows knows whether a match could be cut with it.
use regex_syntax::hir::{Class, Hir, HirKind};

/// The most one match of a pattern can take, and whether it can take lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternReach {
    /// The longest match in bytes; `None` when matches have no bound.
    pub max_len: Option<usize>,
    /// Some match contains a line feed, so it can span lines.
    pub crosses_newline: bool,
}

impl PatternReach {
    /// Nothing is known: unbounded, and able to span lines.
    pub const UNKNOWN: Self = Self {
        max_len: None,
        crosses_newline: true,
    };

    /// Whether one match can be longer than `bytes`.
    pub fn may_exceed(&self, bytes: usize) -> bool {
        self.max_len.is_none_or(|longest| longest > bytes)
    }
}

/// The reach of `pattern` under the flags the Rust-regex scan compiles it
/// with (`^`/`$` per line, CRLF line ends). A pattern outside that syntax
/// (a PCRE2 lookaround, a backreference) has [`PatternReach::UNKNOWN`].
pub fn pattern_reach(pattern: &str, case_insensitive: bool) -> PatternReach {
    let parsed = regex_syntax::ParserBuilder::new()
        .case_insensitive(case_insensitive)
        .multi_line(true)
        .crlf(true)
        .build()
        .parse(pattern);
    match parsed {
        Ok(hir) => PatternReach {
            max_len: hir.properties().maximum_len(),
            crosses_newline: matches_newline(&hir),
        },
        Err(_) => PatternReach::UNKNOWN,
    }
}

/// Whether some literal or class of `hir` takes a line feed.
fn matches_newline(hir: &Hir) -> bool {
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => false,
        HirKind::Literal(literal) => literal.0.contains(&b'\n'),
        HirKind::Class(Class::Unicode(class)) => class
            .ranges()
            .iter()
            .any(|range| range.start() <= '\n' && '\n' <= range.end()),
        HirKind::Class(Class::Bytes(class)) => class
            .ranges()
            .iter()
            .any(|range| range.start() <= b'\n' && b'\n' <= range.end()),
        HirKind::Repetition(repetition) => matches_newline(&repetition.sub),
        HirKind::Capture(capture) => matches_newline(&capture.sub),
        HirKind::Concat(parts) | HirKind::Alternation(parts) => parts.iter().any(matches_newline),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_patterns_do_not_cross_lines() {
        for pattern in [r"fn \w+", r"^b", r"a$", r"a.*b", r"[^\n]+", r"\bfoo\b"] {
            assert!(!pattern_reach(pattern, false).crosses_newline, "{pattern}");
        }
    }

    #[test]
    fn patterns_that_can_take_a_line_feed_cross_lines() {
        for pattern in [
            r"(?s)a.*b",
            r"foo\s+bar",
            r"a\nb",
            r"[^x]",
            r"(?s).",
            r"x|\n",
        ] {
            assert!(pattern_reach(pattern, false).crosses_newline, "{pattern}");
        }
    }

    #[test]
    fn bounds_come_from_the_syntax_tree() {
        assert_eq!(pattern_reach("ab{1,3}c", false).max_len, Some(5));
        assert_eq!(pattern_reach("fn \\w+", false).max_len, None);
        assert!(!pattern_reach("ab{1,3}c", false).may_exceed(5));
        assert!(pattern_reach("ab{1,3}c", false).may_exceed(4));
        assert!(pattern_reach("a+", true).may_exceed(usize::MAX - 1));
    }

    #[test]
    fn syntax_outside_the_rust_grammar_is_unknown() {
        for pattern in [r"(?<=a)b", r"(a)\1", r"a(?=b)"] {
            assert_eq!(pattern_reach(pattern, false), PatternReach::UNKNOWN);
        }
    }
}
