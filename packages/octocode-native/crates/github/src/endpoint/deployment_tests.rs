use super::{Deployment, GitHubEndpoint, api_base_for_host, credential_host};
use url::Url;

/// One deployment kind decides the GraphQL URL, the credential host
/// and the API base of every host a user can name.
#[test]
fn each_host_derives_its_urls_and_credential_host_from_one_kind() {
    for (base, kind, graphql, credentials) in [
        (
            "https://api.github.com",
            Deployment::Cloud,
            "https://api.github.com/graphql",
            "github.com",
        ),
        (
            "https://api.acme.ghe.com",
            Deployment::Cloud,
            "https://api.acme.ghe.com/graphql",
            "acme.ghe.com",
        ),
        (
            "https://ghe.example.com/api/v3",
            Deployment::Server,
            "https://ghe.example.com/api/graphql",
            "ghe.example.com",
        ),
        // A loopback test server keeps the Server layout.
        (
            "http://127.0.0.1:8080/api/v3",
            Deployment::Server,
            "http://127.0.0.1:8080/api/graphql",
            "127.0.0.1",
        ),
    ] {
        let endpoint = GitHubEndpoint::new(Url::parse(base).expect("url")).expect("endpoint");
        assert_eq!(endpoint.deployment(), kind, "{base}");
        assert_eq!(endpoint.graphql().as_str(), graphql, "{base}");
        assert_eq!(endpoint.credential_host(), credentials, "{base}");
    }
}

/// Host strings (a configured host, a login host) follow the same rule.
#[test]
fn host_strings_use_the_same_rule() {
    assert_eq!(credential_host("api.acme.ghe.com"), "acme.ghe.com");
    assert_eq!(credential_host("acme.ghe.com"), "acme.ghe.com");
    assert_eq!(credential_host("api.github.com"), "github.com");
    assert_eq!(
        credential_host("api.ghe.example.com"),
        "api.ghe.example.com"
    );
    // Only one subdomain label is a GHE.com enterprise.
    assert_eq!(Deployment::of_host("api.a.b.ghe.com"), Deployment::Server);
    assert_eq!(Deployment::of_host("api.ghe.com"), Deployment::Server);
    assert_eq!(api_base_for_host("github.com"), "https://api.github.com");
    assert_eq!(
        api_base_for_host("acme.ghe.com"),
        "https://api.acme.ghe.com"
    );
    assert_eq!(
        api_base_for_host("api.acme.ghe.com"),
        "https://api.acme.ghe.com"
    );
    assert_eq!(
        api_base_for_host("ghe.example.com"),
        "https://ghe.example.com/api/v3"
    );
}
