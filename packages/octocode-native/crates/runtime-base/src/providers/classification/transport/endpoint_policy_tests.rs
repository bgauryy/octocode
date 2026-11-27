#[test]
fn judgment_endpoint_uses_the_shared_loopback_rule() {
    for host in [
        "localhost",
        "localhost.",
        "provider.localhost",
        "127.42.0.1",
        "[::1]",
        "example.com",
    ] {
        let base = format!("http://{host}:1234/");
        let url = url::Url::parse(&base).unwrap();
        assert_eq!(
            super::endpoint(&base, "v1/test").is_ok(),
            octocode_github::is_loopback(&url),
            "{host}"
        );
    }
}
