#[test]
fn placeholder_regex_matches_every_emitted_placeholder() {
    let pattern = octocode_engine::security::placeholder_regex();
    for placeholder in [
        octocode_engine::security::redaction_placeholder("githubTokens"),
        "[CONTENT-REDACTED-SIZE-LIMIT]".to_owned(),
        "[CONTENT-REDACTED-SANITIZER-FAILURE]".to_owned(),
    ] {
        assert_eq!(
            pattern.find(&placeholder).map(|found| found.as_str()),
            Some(placeholder.as_str())
        );
    }
}
