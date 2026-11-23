use super::*;

#[test]
fn a_regex_widens_only_past_one_fixed_text_or_with_an_escape() {
    for pattern in [
        "a|b",
        "x[0-9]",
        "^fn",
        "a+",
        "char*",
        "fn .*alpha",
        r"a\.b",
        r"alph\S",
        "(?i)ab",
    ] {
        assert!(regex_widens(pattern), "{pattern}");
    }
    for pattern in [
        "pub(crate) fn",
        "arr[i]",
        "(a)",
        "plain words",
        "defects-queue",
        "call(",
        "",
    ] {
        assert!(!regex_widens(pattern), "{pattern}");
    }
}

#[test]
fn line_breaks_are_named_only_where_the_line_search_rejects_them() {
    assert!(matches_line_break("\\n", false));
    assert!(matches_line_break("a\\nb", false));
    assert!(matches_line_break("a\nb", true));
    assert!(!matches_line_break("\\s+", false));
    assert!(!matches_line_break("\\n", true));
    assert!(!matches_line_break("(", false));
    assert!(!matches_line_break("^fn ", false));
}

#[test]
fn validate_rejects_invalid_default_regex() {
    let result = validate_text_pattern("(", false, false);
    assert!(!result.valid);
    assert!(result.error.is_some());
}

#[test]
fn validate_accepts_fixed_string_without_regex_parsing() {
    let result = validate_text_pattern("(", true, false);
    assert!(result.valid);
}

#[test]
fn validate_compiles_valid_perl_regex() {
    let result = validate_text_pattern("(?<=foo)bar", false, true);
    assert!(result.valid);
}

#[test]
fn validate_rejects_invalid_perl_regex() {
    let result = validate_text_pattern("(?<=", false, true);
    assert!(!result.valid);
    assert!(result.error.is_some());
}

/// Code written as a regex reads with its unpaired brackets as text, the
/// rest kept as the regex it is; a pattern that fails for another reason,
/// or parses, has no such reading.
#[test]
fn unpaired_brackets_read_as_text() {
    for (sent, read) in [
        ("def route(", "def route\\("),
        ("def map[|def imap", "def map\\[|def imap"),
        ("fn main() {", "fn main() \\{"),
        ("foo)", "foo\\)"),
        ("*ptr", "\\*ptr"),
        ("f(a, (b)", "f\\(a, (b)"),
    ] {
        assert_eq!(with_unpaired_as_text(sent).as_deref(), Some(read), "{sent}");
        assert!(validate_text_pattern(read, false, false).valid, "{read}");
        assert!(validate_text_pattern(read, false, true).valid, "{read}");
    }
    for kept in ["^fn ", "(?:x", "a{2,1}"] {
        assert_eq!(with_unpaired_as_text(kept), None, "{kept}");
    }
}
