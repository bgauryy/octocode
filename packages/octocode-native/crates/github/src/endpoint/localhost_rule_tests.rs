use super::{is_loopback, names_localhost, proxy_for};
use octocode_io::http::Proxy;
use url::Url;

/// Every client built for a loopback host (each test's mock server)
/// connects directly; a remote host keeps the user's system proxy.
#[test]
fn only_a_loopback_host_is_reached_without_the_system_proxy() {
    for (url, proxy) in [
        ("http://127.0.0.1:4873/pkg", Proxy::Direct),
        ("http://localhost:1/api/v3", Proxy::Direct),
        ("http://[::1]:8080/", Proxy::Direct),
        ("https://registry.npmjs.org/pkg", Proxy::System),
        ("https://github.com/login/oauth/access_token", Proxy::System),
        ("http://a.localhost/", Proxy::System),
    ] {
        assert_eq!(proxy_for(&Url::parse(url).expect("url")), proxy, "{url}");
    }
}

#[test]
fn the_deny_rule_is_broader_than_the_credential_rule() {
    // Deny: every RFC 6761 `localhost` name, any case, with or without the root dot.
    for host in [
        "localhost",
        "LOCALHOST.",
        "a.localhost",
        "Verdaccio.LocalHost.",
    ] {
        assert!(names_localhost(host), "{host}");
    }
    for host in ["notlocalhost", "localhost.example.com", "example.com", ""] {
        assert!(!names_localhost(host), "{host}");
    }
    // Credentials over HTTP: only the exact loopback names and addresses.
    for url in [
        "http://localhost:4873/",
        "http://127.0.0.2/",
        "http://[::1]/",
    ] {
        assert!(is_loopback(&Url::parse(url).unwrap()), "{url}");
    }
    for url in [
        "http://a.localhost/",
        "http://localhost./",
        "http://10.0.0.1/",
    ] {
        assert!(!is_loopback(&Url::parse(url).unwrap()), "{url}");
    }
}
