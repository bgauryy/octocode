//! Which request strings the input gate refuses as a credential: a search
//! term is one by its value's opacity, never by the name beside it.

use super::*;

fn refused(field: &str, value: serde_json::Value) -> bool {
    !ContentSecurity::new()
        .validate_input_parameters(&serde_json::json!({ field: value }))
        .secret_fields
        .is_empty()
}

#[test]
fn a_search_for_a_credential_named_declaration_runs() {
    for term in [
        "password: str",
        "password: string",
        "token: string",
        "token: Token",
        "api_key: str = None",
        "password = None",
        "secret: Option<String>",
        "apiKey?: string;",
        "password: hunter",
        "const token = \"\"",
        "API_KEY=",
        "token = os.environ[\"GITHUB_TOKEN\"]",
        "secret_key: bytes = b\"\"",
        "password: Annotated[str, Field(min_length=8)]",
    ] {
        for field in ["matchString", "keywords", "questions"] {
            assert!(
                !refused(field, serde_json::json!([term])),
                "{field} {term:?} was refused as a credential"
            );
        }
        assert!(!refused("matchString", serde_json::json!(term)), "{term:?}");
    }
}

#[test]
fn a_pasted_secret_is_still_refused_whatever_names_it() {
    let github = format!("ghp_{}", "a1B2".repeat(9));
    for term in [
        "password: Tr0ub4dor&3xK9!zQ",
        "password = \"Tr0ub4dor&3xK9!zQ\"",
        "token: 9f8Kq2LmZx7Wv3Tn5Rb1Yc6H",
        "api_key: str = \"I5PPdwnIQ10ynyepR7W5CsQV5VFxVbD8\"",
        github.as_str(),
        "AKIAIOSFODNN7EXAMPLE",
    ] {
        for field in ["matchString", "keywords"] {
            assert!(
                refused(field, serde_json::json!([term])),
                "{field} {term:?} was run"
            );
        }
        assert!(refused("matchString", serde_json::json!(term)), "{term:?}");
    }
}

#[test]
fn a_refused_value_is_redacted_where_it_stands() {
    let result = ContentSecurity::new().validate_input_parameters(&serde_json::json!({
        "matchString": ["password: Tr0ub4dor&3xK9!zQ", "password: str"],
    }));
    let kept = serde_json::to_string(&result.sanitized_params).expect("serializable");
    assert!(!kept.contains("Tr0ub4dor"), "{kept}");
    assert!(kept.contains("password: str"), "{kept}");
}
