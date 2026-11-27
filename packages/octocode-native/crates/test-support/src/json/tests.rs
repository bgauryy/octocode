//! `json_fixture` against `json!`: number kinds, string escapes, key order, repeated
//! keys, and the panic for invalid text.
use super::json_fixture;
use serde_json::{Value, json};

/// The same value, and the same text: `Value` equality alone takes `-0.0` for `0.0`.
#[track_caller]
fn same(expected: Value, text: &str) {
    let actual = json_fixture(text);
    assert_eq!(actual, expected);
    assert_eq!(actual.to_string(), expected.to_string());
}

#[test]
fn integers_stay_integers_and_floats_stay_floats() {
    same(
        json!([1, -1, 0, 2147483647, -2147483648]),
        "[1, -1, 0, 2147483647, -2147483648]",
    );
    same(
        json!([1.0, -1.5, 0.1, 1e3, 1.5e-3, 1E300, -0.0]),
        "[1.0, -1.5, 0.1, 1000.0, 0.0015, 1e300, -0.0]",
    );
    assert!(json_fixture("[1]")[0].is_u64());
    assert!(json_fixture("[-1]")[0].is_i64());
    assert!(json_fixture("[1.0]")[0].is_f64());
}

/// `json!(-0)` is the integer 0, but the JSON text `-0` parses as a float.
#[test]
fn negative_zero_is_spelled_zero() {
    same(json!([-0]), "[0]");
    assert!(json_fixture("[-0]")[0].is_f64());
}

#[test]
fn rust_escapes_match_json_escapes() {
    same(
        json!([
            "\u{1F600}",
            "\u{e9}",
            "é",
            "tab\there",
            "line\nbreak",
            "quote\"",
            "back\\slash",
            "nul\0",
            "\x41"
        ]),
        r#"["\ud83d\ude00", "\u00e9", "é", "tab\there", "line\nbreak", "quote\"", "back\\slash", "nul\u0000", "A"]"#,
    );
    same(
        json!([r"raw \n stays", r#"raw "quoted""#]),
        r##"["raw \\n stays", "raw \"quoted\""]"##,
    );
}

#[test]
fn keys_keep_their_order_and_a_repeated_key_its_first_place() {
    same(
        json!({"b": 1, "a": 2, "c": {"z": [], "y": {}}}),
        r#"{"b": 1, "a": 2, "c": {"z": [], "y": {}}}"#,
    );
    same(
        json!({"a": 1, "b": 2, "a": 3}),
        r#"{"a": 1, "b": 2, "a": 3}"#,
    );
    let value = json_fixture(r#"{"a": 1, "b": 2, "a": 3}"#);
    let keys: Vec<&String> = value
        .as_object()
        .into_iter()
        .flat_map(|map| map.keys())
        .collect();
    assert_eq!(keys, ["a", "b"]);
}

#[test]
fn literals_and_nesting() {
    same(
        json!({"t": true, "f": false, "n": null, "s": "", "nested": [[], {}, [null, {"k": [1.5]}]]}),
        r#"{"t": true, "f": false, "n": null, "s": "", "nested": [[], {}, [null, {"k": [1.5]}]]}"#,
    );
}

#[test]
fn invalid_text_panics_naming_the_caller() {
    // The call's line, however it is wrapped, lies between these two.
    let first = line!();
    let panic =
        std::panic::catch_unwind(|| json_fixture(r#"{"a": }"#)).expect_err("invalid JSON panics");
    let last = line!();
    let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
    let prefix = format!("invalid JSON fixture at {}:", file!());
    let line: u32 = message
        .strip_prefix(&prefix)
        .and_then(|rest| rest.split(':').next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("names no caller line: {message}"));
    assert!((first..=last).contains(&line), "{message}");
}
