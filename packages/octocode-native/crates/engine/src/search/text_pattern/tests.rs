use super::*;

#[test]
fn a_regex_spells_itself_only_when_it_parses_to_its_own_text() {
    for pattern in ["defects-queue", "plain words", "#tag", "a&b", "x~y", "é-ü"] {
        assert!(regex_spells_itself(pattern), "{pattern}");
    }
    for pattern in ["a.b", r"a\.b", "a|b", "x[0-9]", "^fn", "a+", "(a)", "call("] {
        assert!(!regex_spells_itself(pattern), "{pattern}");
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
