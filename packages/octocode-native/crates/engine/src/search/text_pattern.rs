use grep_regex::RegexMatcherBuilder;

pub struct TextPatternValidationResult {
    pub valid: bool,
    pub error: Option<String>,
}

#[must_use]
pub fn validate_text_pattern(
    pattern: &str,
    fixed_string: bool,
    perl_regex: bool,
) -> TextPatternValidationResult {
    if pattern.is_empty() {
        return TextPatternValidationResult {
            valid: false,
            error: Some("pattern is empty — provide a non-empty search string".to_owned()),
        };
    }

    if fixed_string {
        return TextPatternValidationResult {
            valid: true,
            error: None,
        };
    }

    if perl_regex {
        return match crate::search::text_search::pcre2_builder().build(pattern) {
            Ok(_) => TextPatternValidationResult {
                valid: true,
                error: None,
            },
            Err(err) => TextPatternValidationResult {
                valid: false,
                error: Some(err.to_string()),
            },
        };
    }

    match RegexMatcherBuilder::new().build(pattern) {
        Ok(_) => TextPatternValidationResult {
            valid: true,
            error: None,
        },
        Err(err) => TextPatternValidationResult {
            valid: false,
            error: Some(err.to_string()),
        },
    }
}

/// Whether `pattern` matches a line break, which the line-by-line search
/// (multiline off, the default engine) rejects: it compiles as a regex but
/// not under the `\n` line terminator that search sets.
#[must_use]
pub fn matches_line_break(pattern: &str, fixed_string: bool) -> bool {
    let pattern = if fixed_string {
        regex::escape(pattern)
    } else {
        pattern.to_owned()
    };
    RegexMatcherBuilder::new().build(&pattern).is_ok()
        && RegexMatcherBuilder::new()
            .crlf(true)
            .line_terminator(Some(b'\n'))
            .build(&pattern)
            .is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
