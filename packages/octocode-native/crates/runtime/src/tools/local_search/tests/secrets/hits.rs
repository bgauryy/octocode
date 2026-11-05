//! A search answers on raw text: every matching line counts, its row shows the
//! redacted line, and a warning says the value is not verbatim. Hiding the hit
//! would claim an absence that is false.
use super::*;
use serde_json::json;

/// What `rg -c` finds of `needle` in `text`.
fn raw_lines(text: &str, needle: &str) -> usize {
    text.lines().filter(|line| line.contains(needle)).count()
}

#[test]
fn a_matching_line_holding_a_secret_is_counted_and_shown_redacted() {
    let token = format!("ghp_{}", "a1B2c3D4".repeat(5));
    let cases = [
        (
            "mark.rs".to_owned(),
            "const S: &str = \"fn beta() -----BEGIN RSA PRIVATE KEY-----\\nabc\";\n".to_owned(),
            "fn beta".to_owned(),
            "[REDACTED-PRIVATEKEYFRAGMENT]",
        ),
        (
            "tok.rs".to_owned(),
            format!("const T: &str = \"{token}\"; // needle\n"),
            "needle".to_owned(),
            "[REDACTED-GITHUBTOKENS]",
        ),
        (
            "tok.rs".to_owned(),
            format!("const T: &str = \"{token}\"; // needle\n"),
            token.clone(),
            "[REDACTED-GITHUBTOKENS]",
        ),
    ];
    for (name, text, needle, placeholder) in &cases {
        let files = [(name.as_str(), text.as_str())];
        let run = |view: &str| {
            search_fixture(
                &files,
                ls_query(json!({"matchString":[needle],"view":view}), None),
            )
        };
        let expected = raw_lines(text, needle);
        assert_eq!(expected, 1, "{needle}");
        let rows = run("content");
        assert_eq!(rows["matchCount"], expected, "{rows}");
        let value = rows["files"][0]["matches"][0]["value"]
            .as_str()
            .expect("a row");
        assert!(value.contains(placeholder), "{value}");
        assert!(!rows.to_string().contains(&token), "{rows}");
        assert!(
            rows["warnings"]
                .as_array()
                .expect("warnings")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .is_some_and(|text| text.starts_with("redactedMatches"))),
            "{rows}"
        );
        for lens in ["countMatches", "countLines"] {
            let count = run(lens);
            assert_eq!(count["matchedLineCount"], expected, "{lens}: {count}");
        }
        assert_eq!(run("files")["files"].as_array().expect("files").len(), 1);
    }
}
