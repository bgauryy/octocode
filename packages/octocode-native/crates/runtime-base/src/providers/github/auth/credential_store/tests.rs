use super::*;
use octocode_test_support::json_fixture;
fn credential() -> StoredCredentials {
    serde_json::from_value(json_fixture(
        r#"{
            "hostname": "example.test",
            "username": "fixture",
            "token": {"token": "synthetic-token", "tokenType": "oauth"},
            "gitProtocol": "https",
            "createdAt": "",
            "updatedAt": ""
        }"#,
    ))
    .unwrap()
}
#[test]
fn home_wins_and_platform_fallback_remains_available() {
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path());
    let fallback = |_: &str| Ok(Some(credential()));
    assert_eq!(
        store
            .load_with("example.test", &fallback)
            .unwrap()
            .unwrap()
            .1,
        CredentialSource::Storage
    );
    store.save(&credential()).unwrap();
    let no_platform =
        |_: &str| -> Result<Option<StoredCredentials>, ProviderError> { panic!("home must win") };
    assert_eq!(
        store
            .load_with("EXAMPLE.TEST", &no_platform)
            .unwrap()
            .unwrap()
            .1,
        CredentialSource::Home
    );
    std::fs::write(dir.path().join("credentials.json"), "corrupt").unwrap();
    assert_eq!(
        store
            .load_with("example.test", &fallback)
            .unwrap()
            .unwrap()
            .1,
        CredentialSource::Storage
    );
    assert!(store.load_with("example.test", &|_| Ok(None)).is_err());
}
#[test]
fn logout_clears_both_sources_and_reports_partial_failure() {
    let dir = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(dir.path());
    store.save(&credential()).unwrap();
    let called = std::cell::Cell::new(false);
    let platform = |host: &str| {
        assert_eq!(host, "example.test");
        called.set(true);
        Err(unavailable())
    };
    assert!(store.delete_with("EXAMPLE.TEST", &platform).is_err());
    assert!(called.get());
    assert!(!dir.path().join("credentials.json").exists());
    assert!(store.delete_with("example.test", &|_| Ok(())).is_ok());
}
