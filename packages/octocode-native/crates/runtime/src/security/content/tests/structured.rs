use super::*;
use crate::security::{InputPolicy, Sanitized};
use serde_json::json;

#[test]
fn sensitive_boundary_input_keeps_structured_secret_context() {
    let secret = "SyntheticCredentialMaterial987654321abcdef";
    let value = json!({"operation":"evaluate","content":{"password":secret},
        "questions":{"q":{"type":"noul","instructions":{"kind":"Secret","data":{"value":secret}}}}});
    let checked = ContentSecurity::new()
        .check_input(
            value,
            InputPolicy::ClasifyEvidence {
                content_limit: crate::id::clasify_policy::REQUEST_BYTES,
            },
        )
        .unwrap();
    assert!(!checked.to_string().contains(secret), "{checked:?}");
    assert_eq!(checked["questions"]["q"]["instructions"]["kind"], "Secret");
}

#[test]
fn sensitive_boundary_output_keeps_structured_secret_context() {
    let secret = "SyntheticCredentialMaterial987654321abcdef";
    let value = json!({"data":{"password":secret,"nested":[{"kty":"RSA","d":secret,"e":"AQAB"}]},
        "path":"src/secrets/credentials/private/source.pem","count":2});
    let shown = Sanitized::sealed(value);
    assert!(!shown.to_string().contains(secret), "{shown}");
    assert_eq!(shown["path"], "src/secrets/credentials/private/source.pem");
    assert_eq!(shown["data"]["nested"][0]["e"], "AQAB");
    assert_eq!(shown["count"], 2);
}

#[test]
fn sensitive_boundary_escaped_secret_values_are_hidden_whole_and_other_data_survives() {
    for secret in [
        "\"leading quote followed by private material 123456789",
        "an apostrophe's private material 123456789",
        "private\\material\nwith\tcontrols\r123456789",
        "🔒秘密_private_material_123456789",
    ] {
        let value = json!({"password":secret,"copy":secret,
            "path":"src/secrets/private/config.ts","public":"keep \"quotes\" and 🔓"});
        let shown = Sanitized::sealed(value);
        assert_ne!(shown["password"], secret, "{shown}");
        assert_ne!(shown["copy"], secret, "{shown}");
        assert_eq!(shown["path"], "src/secrets/private/config.ts");
        assert_eq!(shown["public"], "keep \"quotes\" and 🔓");
        assert_eq!(Sanitized::sealed(shown.clone().into_value()), shown);
    }
}

#[test]
fn sensitive_boundary_locator_members_are_not_secret_data() {
    let token = format!("ghp_{}", "a".repeat(40));
    let value = json!({
        "keys_url":"https://api.github.com/repos/owner/repo/keys{/key_id}",
        "secretPath":"src/secrets/credentials/private/source.pem",
        "credentials_directory":"src/secrets/credentials/private",
        "password_file":"src/private/password.txt",
        "token_path":format!("src/{token}/private"),
        "data":{"password":"SyntheticCredentialMaterial987654321abcdef"}
    });
    let shown = Sanitized::sealed(value.clone());
    for key in [
        "keys_url",
        "secretPath",
        "credentials_directory",
        "password_file",
    ] {
        assert_eq!(shown[key], value[key], "locator {key}");
    }
    assert_ne!(shown["data"]["password"], value["data"]["password"]);
    assert!(!shown["token_path"].as_str().unwrap().contains(&token));
    let text = ContentSecurity::new()
        .sanitize_leaf(&value.to_string(), None)
        .unwrap();
    assert!(text.contains("src/private/password.txt"));
    assert!(text.contains("https://api.github.com/repos/owner/repo/keys{/key_id}"));
}
