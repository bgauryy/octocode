//! Every private-key format through `localSearch`: a body line searched by
//! its own text never comes back, in content or matchOnly.

use super::*;

use octocode_runtime_base::security::key_formats as formats;
use octocode_test_support::json_fixture;

#[test]
fn a_body_line_searched_by_its_text_never_comes_back() {
    for format in formats::formats() {
        for secret in &format.secrets {
            // A whole body line, and a short span of one.
            let span = &secret[..secret.len().min(12)];
            for (needle, view) in [
                (secret.as_str(), "content"),
                (secret.as_str(), "matchOnly"),
                (span, "content"),
                (span, "matchOnly"),
            ] {
                let found = search_fixture(
                    &[("notes.txt", format.text.as_str())],
                    ls_query(
                        serde_json::json!({"matchString":[needle],"view":view}),
                        None,
                    ),
                );
                let values = all_values(&found);
                for hidden in &format.secrets {
                    assert!(
                        !values.contains(hidden.as_str()) && !values.contains(span),
                        "{} {view}: {found}",
                        format.name
                    );
                }
            }
        }
    }
}

#[test]
fn a_late_rfc4716_body_in_a_large_source_never_comes_back() {
    let all = formats::formats();
    let format = all
        .iter()
        .find(|format| format.name == "rfc4716 encrypted")
        .expect("format");
    let filler = "let filler_value = 0;\n".repeat(560_000);
    let content = format!("{filler}{}", format.text);
    for view in ["matchOnly", "content"] {
        let found = search_fixture(
            &[("big.txt", content.as_str())],
            ls_query(
                serde_json::json!({"matchString":[format.secrets[1]],"view":view}),
                None,
            ),
        );
        assert!(!all_values(&found).contains(&format.secrets[1]), "{found}");
    }
}

/// Lines around a key stay readable when matched, in the file that holds it.
#[test]
fn a_near_miss_marker_leaves_its_lines_searchable() {
    let run = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5QUJDREVGR0hJSktM";
    for miss in formats::NOT_MARKERS {
        let file = format!("{miss}\n{run}\n");
        let found = search_fixture(
            &[("notes.txt", file.as_str())],
            ls_query(
                serde_json::json!({"matchString":[run],"view":"content"}),
                None,
            ),
        );
        assert!(all_values(&found).contains(run), "{miss:?}: {found}");
    }
}

/// A span inside a key block shows the placeholder a read of its line shows,
/// in every span view, and the body a read hides makes no base64 hit; base64
/// outside the block shows as written.
#[test]
fn a_span_in_a_key_block_shows_the_placeholder_a_read_shows() {
    let body = "WzHvuy3fEWJr5musirUr5JYe5CMpm8DI2mKvqGsxgG5oDTudHZGh0Z7tcHL5kVeqJy";
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    let file = format!(
        "logo: {png}\nplain\n-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n-----END OPENSSH PRIVATE KEY-----\nordinary tail text\n"
    );
    let placeholder = octocode_runtime_base::security::key_fragment_placeholder();
    for query in [
        json_fixture(
            r#"{
                "matchString": ["[A-Za-z0-9+/=]{30,}"],
                "regexEngine": "rust",
                "view": "matchOnly"
            }"#,
        ),
        json_fixture(
            r#"{
                "matchString": ["[A-Za-z0-9+/=]{30,}"],
                "regexEngine": "rust",
                "view": "matchOnly",
                "unique": "list"
            }"#,
        ),
        json_fixture(
            r#"{
                "matchString": ["KEY-----\\n.*\\n.*"],
                "regexEngine": "rust",
                "multiline": "on",
                "view": "matchOnly"
            }"#,
        ),
        json_fixture(
            r#"{
                "matchString": ["(?<=^)[A-Za-z0-9+/=]{30,}"],
                "regexEngine": "pcre2",
                "view": "matchOnly"
            }"#,
        ),
    ] {
        let found = search_fixture(
            &[("notes.txt", file.as_str())],
            ls_query(query.clone(), None),
        );
        let rows = found["files"][0]["matches"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let in_block: Vec<_> = rows
            .iter()
            .filter(|row| (3..=6).contains(&row["line"].as_u64().expect("line")))
            .collect();
        // A body a read hides is no base64 run: only a pattern that matches
        // the read's own text of the block (its armor, its placeholders) has
        // a row there.
        assert_eq!(
            !in_block.is_empty(),
            query["multiline"] == "on",
            "{query}: {found}"
        );
        // A row's lines in the block show the placeholder, an armor line
        // shows what a read of it shows, and a line past the block reads as
        // written.
        let armor = |line: &str| {
            !line.is_empty()
                && [
                    "-----BEGIN OPENSSH PRIVATE KEY-----",
                    "-----END OPENSSH PRIVATE KEY-----",
                ]
                .iter()
                .any(|marker| marker.ends_with(line))
        };
        for row in in_block {
            let value = row["value"].as_str().expect("value");
            assert!(
                value
                    .lines()
                    .all(|line| line == placeholder || armor(line) || line == "ordinary tail text"),
                "{query}: {found}"
            );
        }
        for row in rows.iter().filter(|row| row["line"] == 1) {
            assert_eq!(row["value"], png, "{query}: {found}");
        }
        assert!(
            !all_values(&found).contains(&body[..12]),
            "{query}: {found}"
        );
    }
}
