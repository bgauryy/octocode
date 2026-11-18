use super::*;
use crate::security::Sanitized;
use serde_json::json;

/// No member is judged by its name: a locator named for a credential and a
/// password in no known format stay, and a known token goes from any member.
#[test]
fn members_are_judged_by_their_values_alone() {
    let token = format!("ghp_{}", "a".repeat(40));
    let value = json!({
        "keys_url":"https://api.github.com/repos/owner/repo/keys{/key_id}",
        "secretPath":"src/secrets/credentials/private/source.pem",
        "credentials_directory":"src/secrets/credentials/private",
        "password_file":"src/private/password.txt",
        "token_path":format!("src/{token}/private"),
        "data":{"password":"SyntheticCredentialMaterial987654321abcdef","token":token}
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
    assert_eq!(shown["data"]["password"], value["data"]["password"]);
    assert!(!shown.to_string().contains(&token), "{shown}");
    let text = ContentSecurity::new()
        .sanitize_leaf(&value.to_string())
        .unwrap();
    assert!(text.contains("src/private/password.txt"));
    assert!(text.contains("https://api.github.com/repos/owner/repo/keys{/key_id}"));
    assert!(!text.contains(&token));
}
