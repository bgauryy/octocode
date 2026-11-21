//! An astSearch row's `content` is inline source, not the caller's own words:
//! bounded by the contract's limit of the field, never by a query string's,
//! and redacted in place where a caller's word would be refused.
use super::*;
use crate::id::ToolId;
use serde_json::json;

/// A fake GitHub token: `ghp_` and 36 alphanumerics.
const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

fn policy() -> InputPolicy {
    InputPolicy::of(ToolId::AstSearch)
}

fn limit() -> usize {
    match policy() {
        InputPolicy::LocalSnippet { content_limit } => content_limit,
        other => panic!("astSearch rows take inline content: {other:?}"),
    }
}

fn row(code: &str) -> Value {
    json!({"operation":"syntaxTree","content":code,"parser":"typescript"})
}

/// Only the tool whose contract bounds its inline `content` takes it as source.
#[test]
fn only_ast_search_rows_hold_inline_source() {
    for tool in ToolId::ALL {
        let snippet = matches!(InputPolicy::of(tool), InputPolicy::LocalSnippet { .. });
        assert_eq!(snippet, tool == ToolId::AstSearch, "{tool:?}");
    }
    assert!(
        limit() > MAX_STRING_LENGTH,
        "a snippet outgrows a query string"
    );
}

#[test]
fn a_credential_in_code_is_redacted_in_place_and_is_no_admitted_text() {
    let security = ContentSecurity::new();
    let code = format!("const key = \"{TOKEN}\";\nuse(key);\n");
    let checked = security
        .check_input(row(&code), policy())
        .expect("a snippet holding a credential runs");
    let shown = checked["content"].as_str().expect("content");
    assert!(!shown.contains(TOKEN), "{shown}");
    assert!(shown.starts_with("const key = \""), "{shown}");
    assert!(shown.ends_with("\";\nuse(key);\n"), "{shown}");
    // Nothing of the snippet is the caller's own text an output may restore.
    let own = Admitted::of([&checked]);
    assert!(!own.occurs_in(shown), "{own:?}");
    assert!(own.occurs_in("typescript"));
    // The gate is the same on the shown text: a saved row opens unchanged.
    let again = security
        .check_saved_input(checked.clone().into_value(), policy(), ToolId::AstSearch)
        .expect("a saved snippet row");
    assert_eq!(again["content"], checked["content"]);
}

#[test]
fn a_credential_beside_code_is_still_refused() {
    let security = ContentSecurity::new();
    let mut with_rule = row("foo(1)");
    with_rule["rule"] = json!({"pattern": TOKEN});
    let issues = security
        .check_input(with_rule, policy())
        .expect_err("a rule's own text is the caller's");
    assert!(
        issues
            .iter()
            .any(|issue| issue.rule_id == "security.credential"),
        "{issues:?}"
    );
    // Only the row's own `content`: one nested in another field is a word.
    let nested = json!({"operation":"match","path":"src","rule":{"content": TOKEN}});
    assert!(security.check_input(nested, policy()).is_err());
    // A path of this machine may still spell one.
    let path = json!({"operation":"syntaxTree","path": format!("deploy-{TOKEN}.ts")});
    assert!(security.check_input(path, policy()).is_ok());
}

#[test]
fn code_is_bounded_by_its_own_limit_in_characters() {
    let security = ContentSecurity::new();
    // Escapes and astral characters count once each, as the contract counts.
    let line = "f(\"\u{1F600}\");\n";
    let unit = line.chars().count();
    let within = line.repeat(limit() / unit);
    assert!(within.chars().count() > MAX_STRING_LENGTH);
    assert!(security.check_input(row(&within), policy()).is_ok());
    let over = "a".repeat(limit() + 1);
    let issues = security
        .check_input(row(&over), policy())
        .expect_err("past the limit");
    assert!(
        issues
            .iter()
            .any(|issue| issue.message.contains(&limit().to_string())
                && issue.message.contains("content")),
        "{issues:?}"
    );
    // Every other string keeps a query string's bound.
    let mut long_goal = row("foo(1)");
    long_goal["mainGoal"] = json!("a".repeat(MAX_STRING_LENGTH + 1));
    assert!(security.check_input(long_goal, policy()).is_err());
}
