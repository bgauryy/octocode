use super::configured_credential_host;

#[test]
fn configured_credential_host_maps_api_github_to_github_com() {
    assert_eq!(
        configured_credential_host("https://api.github.com").as_deref(),
        Some("github.com")
    );
    assert_eq!(
        configured_credential_host("https://ghe.example.com/api/v3").as_deref(),
        Some("ghe.example.com")
    );
    assert_eq!(configured_credential_host("not a url"), None);
}
