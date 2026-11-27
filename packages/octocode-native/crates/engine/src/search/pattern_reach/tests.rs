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
