#[test]
fn placeholder_regex_matches_every_emitted_placeholder() {
    for placeholder in super::patterns::PATTERNS
        .iter()
        .map(|p| super::redaction_placeholder(p.name))
        .chain([super::SANITIZER_FAILURE_PLACEHOLDER].map(str::to_owned))
    {
        assert_eq!(
            super::placeholder_regex()
                .find(&placeholder)
                .map(|found| found.as_str()),
            Some(placeholder.as_str())
        );
    }
}
