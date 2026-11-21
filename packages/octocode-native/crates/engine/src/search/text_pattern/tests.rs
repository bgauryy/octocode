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
