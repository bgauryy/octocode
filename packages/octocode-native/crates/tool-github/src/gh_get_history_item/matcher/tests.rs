use super::*;
use octocode_test_support::json_fixture;

#[test]
fn history_uses_the_shared_matcher_for_every_match_variant() {
    for identity in [
        json_fixture(r#"{"operation": "pullRequest", "number": 1}"#),
        json_fixture(r#"{"operation": "commit", "ref": "abc"}"#),
        json_fixture(
            r#"{
                    "operation": "compare",
                    "baseRef": "base",
                    "headRef": "head"
                }"#,
        ),
    ] {
        for (options, hit, miss) in [
            (
                json_fixture(r#"{"matchString": ["Token", "other"]}"#),
                "Token",
                "token",
            ),
            (
                json_fixture(r#"{"matchString": ["token"]}"#),
                "TOKEN",
                "toke",
            ),
            (
                json_fixture(r#"{"matchString": ["token"]}"#),
                "tokenized",
                "to ken",
            ),
            (
                json_fixture(r#"{"matchString": ["TOKEN"], "caseMode": "insensitive"}"#),
                "token",
                "toke",
            ),
            (
                json_fixture(r#"{"matchString": ["token"], "caseMode": "sensitive"}"#),
                "token",
                "TOKEN",
            ),
            (
                json_fixture(r#"{"matchString": ["tok.n"], "regexEngine": "literal"}"#),
                "tok.n",
                "token",
            ),
            (
                json_fixture(r#"{"matchString": ["tok.n"], "regexEngine": "rust"}"#),
                "token",
                "tokn",
            ),
            (
                json_fixture(r#"{"matchString": ["(?<=pre)token"], "regexEngine": "pcre2"}"#),
                "pretoken",
                "token",
            ),
        ] {
            let mut row = json_fixture(r#"{"owner": "a", "repo": "b"}"#);
            for fields in [&identity, &options] {
                for (key, value) in fields.as_object().expect("fields") {
                    row[key] = value.clone();
                }
            }
            let typed = serde_json::from_value(row).expect("typed match variant");
            let matcher = Matcher::of(&typed)
                .expect("valid matcher")
                .expect("match supplied");
            assert!(
                matcher.matches(hit),
                "{identity}: {options} must match {hit}"
            );
            assert!(
                !matcher.matches(miss),
                "{identity}: {options} must reject {miss}"
            );
            matcher.result().expect("complete match evaluation");
        }
    }
}

/// An invalid pattern is not a type error of the whole row: the row builds,
/// and its validation error names the field and the engine's reason.
#[test]
fn an_invalid_pattern_names_the_field_and_the_reason() {
    let row = json_fixture(
        r#"{"operation": "pullRequest", "owner": "o", "repo": "r", "number": 1,
            "matchString": ["(unclosed"], "regexEngine": "rust"}"#,
    );
    let request = crate::gh_get_history_item::HistoryItemRequest::from_row(row).expect("row");
    let error = request
        .matcher
        .as_ref()
        .expect("matcher")
        .result()
        .expect_err("invalid");
    assert!(error.message.contains("matchString"), "{}", error.message);
    assert!(error.message.len() > "Invalid `matchString`: ".len());
}
