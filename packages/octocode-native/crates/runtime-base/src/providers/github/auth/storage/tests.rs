use super::*;
use octocode_test_support::json_fixture;

#[test]
fn stored_credentials_debug_never_exposes_access_or_refresh_tokens() {
    let stored: StoredCredentials = serde_json::from_value(json_fixture(
        r#"{
            "hostname": "example.test",
            "username": "fixture-user",
            "token": {
                "token": "synthetic-access-secret",
                "tokenType": "oauth",
                "refreshToken": "synthetic-refresh-secret"
            },
            "gitProtocol": "https",
            "createdAt": "",
            "updatedAt": ""
        }"#,
    ))
    .unwrap();
    assert!(!format!("{stored:?}").contains("synthetic-"));
    assert!(!format!("{:?}", stored.token).contains("synthetic-"));
}

#[test]
fn only_a_json_record_parses_as_stored_credentials() {
    let stored = StoredCredentials {
        hostname: "example.test".into(),
        username: "fixture-user".into(),
        token: OAuthToken {
            token: "synthetic-token".into(),
            token_type: "oauth".into(),
            scopes: Some(vec!["repo".into()]),
            refresh_token: None,
            expires_at: None,
            refresh_token_expires_at: None,
        },
        git_protocol: "ssh".into(),
        created_at: "2026-01-01".into(),
        updated_at: "2026-01-02".into(),
    };
    let blob = serde_json::to_string(&stored).unwrap();
    assert_eq!(parse_stored_blob(&blob), Some(stored));
    assert_eq!(parse_stored_blob("  gho_x \n"), None);
    assert_eq!(parse_stored_blob("   "), None);
}
