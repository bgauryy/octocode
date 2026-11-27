//! A repair names only what the branch the value selects accepts, and only
//! fields a caller authors: never another branch's enum values, never a
//! field that only a continuation supplies (`x-cursor`).

use super::*;

fn messages(tool: &str, query: Value) -> Vec<String> {
    prepare_rows(tool, json!({ "queries": [query] }))
        .expect_err("the query is refused")
        .issues
        .into_iter()
        .map(|issue| format!("{}: {}", issue.path.join("."), issue.message))
        .collect()
}

/// The enum list for a wrong value comes from the branch the operation
/// selects, not from the union of branches that share the field name.
#[test]
fn an_enum_list_comes_from_the_selected_operation() {
    let lines = messages("graphQuery", json!({"operation":"stats","view":"bogus"}));
    let line = lines
        .iter()
        .find(|line| line.starts_with("queries.0.view:"))
        .unwrap_or_else(|| panic!("a view issue: {lines:?}"));
    assert!(line.contains("allowed: summary, details"), "{line}");
    assert!(
        !line.contains("files") && !line.contains("modules"),
        "{line}"
    );
}

/// A missing-field repair never offers a field only a next call supplies.
#[test]
fn a_missing_field_repair_names_authored_alternatives() {
    let lines = messages("clasify", json!({"operation":"sections"}));
    let joined = lines.join("\n");
    assert!(!joined.contains("assessment"), "{joined}");
    assert!(
        joined.contains("resources") && joined.contains("questions"),
        "{joined}"
    );
}

/// An operation some branch accepts together with a field that branch
/// declares: an out-of-enum value is blamed on that field, listing that
/// branch's values, whichever other branch lacks the field.
#[test]
fn an_out_of_enum_value_is_blamed_on_the_branch_declaring_the_field() {
    for operation in ["deps", "dependents", "callers"] {
        let lines = messages(
            "graphQuery",
            json!({"operation":operation,"target":"src/lib.rs","view":"bogus","graph":"G"}),
        );
        let joined = lines.join("\n");
        let line = lines
            .iter()
            .find(|line| line.starts_with("queries.0.view:"))
            .unwrap_or_else(|| panic!("{operation}: a view issue: {lines:?}"));
        assert!(line.contains("outside the allowed enum"), "{line}");
        assert!(line.contains("files"), "{operation}: {line}");
        assert!(!line.contains("summary"), "{operation}: {line}");
        assert!(!joined.contains("requires operation"), "{joined}");
    }
}

/// A field's "requires operation" list names every operation whose branch
/// declares the field, however the forms that declare it are grouped.
#[test]
fn a_requires_operation_list_names_every_declaring_operation() {
    for operation in ["hubs", "callees", "walk"] {
        let error = prepare_rows(
            "graphQuery",
            json!({"queries":[{"operation":operation,"graph":"G","view":"files"}]}),
        )
        .expect_err("the field is refused");
        let shown = crate::contracts::validate::format_input_error(
            "graphQuery",
            &error,
            &serde_json::Value::Null,
        );
        let text = shown["error"].as_str().unwrap_or_default();
        for declaring in ["stats", "deps", "dependents", "callers"] {
            assert!(
                text.contains(&format!("\"{declaring}\"")),
                "{operation}: {text}"
            );
        }
    }
}
