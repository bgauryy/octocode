//! One line longer than the scanner ever held at once: its hits read as the text
//! they are, with only a secret's own bytes hidden. Every value below is fake.
use super::*;
use octocode_test_support::json_fixture;

const TOKEN: &str = "ghp_aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1gI3kM5";
const MIB: usize = 1024 * 1024;

fn one_line(size: usize, at_middle: &str) -> String {
    let unit = "word word word word word word word word\n".replace('\n', " ");
    let half = unit.repeat(size / 2 / unit.len());
    format!("{half}{at_middle}{half}\n")
}

fn rows(text: &str, fields: serde_json::Value) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture directory");
    let path = root.path().join("one.txt");
    fs::write(&path, text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let mut fields = fields;
    fields["path"] = serde_json::json!(path.to_string_lossy());
    search_in(&ls_query(fields, None), &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize")
}

fn first_value(found: &serde_json::Value) -> String {
    found["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("a value")
        .to_owned()
}

/// The size limit never reads as a secret: the hit is text, around its own place.
#[test]
fn a_hit_on_a_long_line_shows_its_text() {
    for size in [10 * MIB + 1, 11 * MIB, 65 * MIB] {
        let found = rows(
            &one_line(size, " the needle here "),
            json_fixture(r#"{"matchString": ["needle"]}"#),
        );
        let value = first_value(&found);
        assert!(value.contains("the needle here"), "{size}: {value:.200}");
        assert!(
            value.len() < MIB,
            "{size}: a row holds a window of the line"
        );
        assert!(!found.to_string().contains("REDACTED"), "{size}");
        assert!(!found.to_string().contains("\\u0001redacted"), "{size}");
    }
}

/// A secret beside the hit goes, and only the secret: the words around it stay.
#[test]
fn a_secret_on_a_long_line_hides_only_its_own_bytes() {
    let found = rows(
        &one_line(11 * MIB, &format!(" token={TOKEN} the needle here ")),
        json_fixture(r#"{"matchString": ["needle"]}"#),
    );
    let value = first_value(&found);
    assert!(!value.contains(&TOKEN[8..24]), "{value:.400}");
    assert!(value.contains("the needle here"), "{value:.400}");
    assert!(value.contains("word word"), "{value:.400}");
    assert!(value.contains("[REDACTED-"), "{value:.400}");
}

/// A span mode's rows, on a line far over the limit: the text a read shows is a
/// row, and the secret's bytes are none.
#[test]
fn spans_on_a_long_line_hide_only_what_a_secret_holds() {
    let found = rows(
        &one_line(11 * MIB, &format!(" needle {TOKEN} needle ")),
        json_fixture(
            r#"{
                "matchString": ["needle", "ghp_aB3d"],
                "view": "matchOnly",
                "limit": 3
            }"#,
        ),
    );
    let values = found["files"][0]["matches"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["value"].as_str().expect("value").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(values, ["needle", "needle"], "{found}");
}

/// Below the limit a row of a line too long for it is the line's own text
/// around the hit, placed by its column; the line whole is a read's.
#[test]
fn a_line_within_the_limit_shows_its_text_around_the_hit() {
    let text = one_line(9 * MIB, " the needle here ");
    let found = rows(&text, json_fixture(r#"{"matchString": ["needle"]}"#));
    let hit = text.find("needle").expect("hit");
    let margin = octocode_tools_shared::source_view::extraction::MATCH_MARGIN;
    assert_eq!(
        first_value(&found),
        text[hit - margin..hit + "needle".len() + margin]
    );
    assert_eq!(found["files"][0]["matches"][0]["column"], hit + 1);
}

/// A span that selects part of a secret loses only the secret's bytes: what it
/// selected beside them stays.
#[test]
fn a_span_keeps_what_is_not_the_secret() {
    let text = format!("before needle-{TOKEN} after\n");
    // The token is text a read withholds: a span reaching into it is no hit.
    let hidden = rows(
        &text,
        json_fixture(r#"{"matchString": ["needle-ghp_aB3d"], "view": "matchOnly"}"#),
    );
    assert_eq!(hidden["files"], serde_json::json!([]), "{hidden}");
    let found = rows(
        &text,
        json_fixture(
            r#"{
                "matchString": ["needle-[REDACTED-"],
                "regexEngine": "literal",
                "view": "matchOnly"
            }"#,
        ),
    );
    let value = first_value(&found);
    assert!(!value.contains("ghp_aB3d"), "{value}");
    assert!(value.starts_with("needle-[REDACTED-"), "{value}");
}
