//! Rust-owned secret detection and content sanitization.
//!
//! `patterns.rs` joins the canonical ordered pattern set; `build.rs` derives the
//! detector's prescan gate from it.

pub(crate) mod detector;
pub(crate) mod patterns;
pub mod sanitizer;
pub mod types;

/// Prefix of detector placeholders (`[REDACTED-<NAME>]`).
pub const REDACTION_PREFIX: &str = "[REDACTED";

/// The placeholder that stands in for a secret of kind `name`.
pub fn redaction_placeholder(name: &str) -> String {
    format!("{REDACTION_PREFIX}-{}]", name.to_ascii_uppercase())
}

/// Prefix of placeholders replacing a whole content view.
pub const CONTENT_REDACTION_PREFIX: &str = "[CONTENT-REDACTED";

/// The whole-content placeholder when sanitizing failed: content fails closed.
pub const SANITIZER_FAILURE_PLACEHOLDER: &str = "[CONTENT-REDACTED-SANITIZER-FAILURE]";

/// Every detector and whole-content redaction placeholder.
#[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
pub fn placeholder_regex() -> &'static regex::Regex {
    static PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(&format!(
            r"(?:{}|{})-[A-Z0-9_-]+\]",
            regex::escape(REDACTION_PREFIX),
            regex::escape(CONTENT_REDACTION_PREFIX)
        ))
        .expect("placeholder regex")
    });
    &PATTERN
}

#[cfg(test)]
mod placeholder_tests {
    #[test]
    fn placeholder_regex_matches_every_emitted_placeholder() {
        for placeholder in super::patterns::PATTERNS
            .iter()
            .map(|p| super::redaction_placeholder(p.name))
            .chain(
                [
                    super::detector::CONTENT_SIZE_LIMIT_PLACEHOLDER,
                    super::SANITIZER_FAILURE_PLACEHOLDER,
                ]
                .map(str::to_owned),
            )
        {
            assert_eq!(
                super::placeholder_regex()
                    .find(&placeholder)
                    .map(|found| found.as_str()),
                Some(placeholder.as_str())
            );
        }
    }
}
