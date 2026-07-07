use grep::pcre2::RegexMatcherBuilder as Pcre2MatcherBuilder;
use grep::regex::RegexMatcherBuilder;
use napi_derive::napi;

#[napi(object)]
pub struct RipgrepPatternValidationResult {
    pub valid: bool,
    pub error: Option<String>,
}

pub fn validate(
    pattern: &str,
    fixed_string: bool,
    perl_regex: bool,
) -> RipgrepPatternValidationResult {
    if pattern.is_empty() {
        return RipgrepPatternValidationResult {
            valid: false,
            error: Some("pattern is empty — provide a non-empty search string".to_owned()),
        };
    }

    if fixed_string {
        return RipgrepPatternValidationResult {
            valid: true,
            error: None,
        };
    }

    if perl_regex {
        let mut builder = Pcre2MatcherBuilder::new();
        builder
            .utf(true)
            .ucp(true)
            .jit_if_available(true)
            .max_jit_stack_size(Some(crate::ripgrep_search::PCRE2_MAX_JIT_STACK_BYTES));
        return match builder.build(pattern) {
            Ok(_) => RipgrepPatternValidationResult {
                valid: true,
                error: None,
            },
            Err(err) => RipgrepPatternValidationResult {
                valid: false,
                error: Some(err.to_string()),
            },
        };
    }

    match RegexMatcherBuilder::new().build(pattern) {
        Ok(_) => RipgrepPatternValidationResult {
            valid: true,
            error: None,
        },
        Err(err) => RipgrepPatternValidationResult {
            valid: false,
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_invalid_default_ripgrep_regex() {
        let result = validate("(", false, false);
        assert!(!result.valid);
        assert!(result.error.is_some());
    }

    #[test]
    fn validate_accepts_fixed_string_without_regex_parsing() {
        let result = validate("(", true, false);
        assert!(result.valid);
    }

    #[test]
    fn validate_compiles_valid_perl_regex() {
        let result = validate("(?<=foo)bar", false, true);
        assert!(result.valid);
    }

    #[test]
    fn validate_rejects_invalid_perl_regex() {
        let result = validate("(?<=", false, true);
        assert!(!result.valid);
        assert!(result.error.is_some());
    }
}
