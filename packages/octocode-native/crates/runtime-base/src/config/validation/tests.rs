use super::config_issues;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// The issue messages `config_issues` reports for `config`.
fn errors(config: &Value) -> Vec<String> {
    config_issues(config)
        .0
        .into_iter()
        .map(|issue| issue.message)
        .collect()
}

#[test]
fn home_paths_need_a_separator_after_the_tilde() {
    for (path, valid) in [
        ("~", true),
        ("~/x", true),
        ("/abs", true),
        ("~user/x", false),
    ] {
        let errors = errors(&json!({"local": {"workspaceRoot": path}}));
        assert_eq!(errors.is_empty(), valid, "{path}");
    }
}

#[test]
fn enum_values_and_null_validate_clean() {
    for value in [json!("targeted"), json!("off"), Value::Null] {
        let errors = errors(&json!({"lsp": {"prewarm": value}}));
        assert!(errors.is_empty(), "{value}: unexpected errors: {errors:?}");
    }
}

#[test]
fn enum_invalid_string_errors() {
    let errors = errors(&json_fixture(r#"{"lsp": {"prewarm": "xml"}}"#));
    assert!(
        errors
            .iter()
            .any(|error| error == "lsp.prewarm: Must be one of: targeted, all, off"),
        "{errors:?}"
    );
}

#[test]
fn enum_non_string_errors() {
    let errors = errors(&json_fixture(r#"{"lsp": {"prewarm": 42}}"#));
    assert!(
        errors
            .iter()
            .any(|error| error == "lsp.prewarm: Must be a string"),
        "{errors:?}"
    );
}

/// A retired setting is ignored, never an error.
#[test]
fn the_retired_output_format_validates_clean() {
    let errors = errors(&json_fixture(r#"{"output": {"format": "yaml"}}"#));
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
}
