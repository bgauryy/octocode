//! The git web root a clone comes from follows the endpoint's deployment
//! kind: GitHub's cloud clones from the credential host at its root, an
//! Enterprise Server from the host and prefix before `/api/v3`.
use super::*;

fn clone_url(base: &str) -> Result<String, CloneError> {
    let endpoint = GitHubEndpoint::new(url::Url::parse(base).expect("URL")).expect("endpoint");
    repository_url(&web_root(&endpoint)?, "owner", "repo")
}

#[test]
fn each_deployment_clones_from_its_web_host() {
    for (base, expected) in [
        (
            "https://api.github.com",
            "https://github.com/owner/repo.git",
        ),
        (
            "https://api.github.com/",
            "https://github.com/owner/repo.git",
        ),
        // GHE.com mirrors github.com: the tenant's web host, no /api/v3.
        (
            "https://api.acme.ghe.com",
            "https://acme.ghe.com/owner/repo.git",
        ),
        (
            "https://API.Acme.GHE.com/",
            "https://acme.ghe.com/owner/repo.git",
        ),
        (
            "https://ghe.example.com/api/v3",
            "https://ghe.example.com/owner/repo.git",
        ),
        (
            "https://ghe.example.com/prefix/api/v3/",
            "https://ghe.example.com/prefix/owner/repo.git",
        ),
        (
            "http://127.0.0.1:8080/api/v3",
            "http://127.0.0.1:8080/owner/repo.git",
        ),
    ] {
        assert_eq!(clone_url(base).expect(base), expected, "{base}");
    }
}

#[test]
fn an_endpoint_with_no_known_web_root_is_refused() {
    for base in [
        // A Server endpoint not ending in /api/v3.
        "https://ghe.example.com/api",
        "https://proxy.example.com/",
        // A cloud host with a path, or a cloud web host named as the API.
        "https://api.github.com/api/v3",
        "https://api.acme.ghe.com/v3",
        "https://github.com",
        "https://acme.ghe.com",
        // A cloud API host with a port: its git host has no such port.
        "https://api.github.com:8443",
        "https://api.acme.ghe.com:8443",
    ] {
        let error = clone_url(base).expect_err(base);
        assert_eq!(error.code, ErrorCode::Config, "{base}");
    }
}
