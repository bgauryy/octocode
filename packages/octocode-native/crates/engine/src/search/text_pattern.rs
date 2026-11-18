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

/// Whether `pattern`, read as a regex, matches exactly the text it spells:
/// it parses to that one literal. Escaping marks characters that are literal
/// outside a class (`-`, `#`, `&`, `~`), so `a-b` holds no regex syntax by
/// this reading, while `a.b` and `a\.b` each read as other text.
#[must_use]
pub fn regex_spells_itself(pattern: &str) -> bool {
    use regex_syntax::hir::HirKind;
    regex_syntax::parse(pattern).is_ok_and(|hir| match hir.kind() {
        HirKind::Literal(literal) => *literal.0 == *pattern.as_bytes(),
        HirKind::Empty => pattern.is_empty(),
        _ => false,
    })
}

#[cfg(test)]
mod tests;
