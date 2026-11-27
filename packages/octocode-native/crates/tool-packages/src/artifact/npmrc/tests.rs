use super::authorization_for;
use secrecy::ExposeSecret;
use url::Url;

fn auth(registry: &str, npmrc: &str) -> Option<String> {
    let env = |name: &str| (name == "NPM_TOKEN").then(|| "from-env".to_owned());
    authorization_for(&Url::parse(registry).unwrap(), npmrc, env)
        .map(|secret| secret.header.expose_secret().to_owned())
}

#[test]
fn npmrc_withholds_auth_over_http_to_a_localhost_subdomain() {
    for host in ["verdaccio.localhost", "localhost."] {
        let npmrc = format!("//{host}:4873/:_authToken=local\n");
        assert_eq!(
            auth(&format!("http://{host}:4873/"), &npmrc),
            None,
            "{host}"
        );
        assert_eq!(
            auth(&format!("https://{host}:4873/"), &npmrc).as_deref(),
            Some("Bearer local")
        );
    }
}

#[test]
fn token_is_sent_only_to_its_scoped_origin() {
    let npmrc = "//npm.example.com/:_authToken=abc\n";
    assert_eq!(
        auth("https://npm.example.com/", npmrc).as_deref(),
        Some("Bearer abc")
    );
    assert_eq!(
        auth("https://npm.example.com/sub/path", npmrc).as_deref(),
        Some("Bearer abc")
    );
    // Different host, suffix host, or port never receives the token.
    for other in [
        "https://evil.example/",
        "https://npm.example.com.evil.example/",
        "https://npm.example.com:8443/",
        "https://registry.npmjs.org/",
    ] {
        assert_eq!(auth(other, npmrc), None, "{other}");
    }
}

/// npmrc keys carry no scheme, so a query naming `http://` for the same
/// host must not receive the token in clear text. Only loopback (a local
/// Verdaccio) may use plain HTTP.
#[test]
fn token_is_never_sent_over_plain_http_off_loopback() {
    let npmrc = "//npm.corp.example/:_authToken=abc\n//localhost:4873/:_authToken=local\n//[::1]:4873/:_authToken=v6\n";
    assert_eq!(auth("http://npm.corp.example/", npmrc), None);
    assert_eq!(
        auth("https://npm.corp.example/", npmrc).as_deref(),
        Some("Bearer abc")
    );
    assert_eq!(
        auth("http://localhost:4873/", npmrc).as_deref(),
        Some("Bearer local")
    );
    assert_eq!(
        auth("http://[::1]:4873/", npmrc).as_deref(),
        Some("Bearer v6")
    );
}

#[test]
fn path_scoped_tokens_prefer_the_longest_match() {
    let npmrc = "//h.test/:_authToken=root\n//h.test/api/npm/:_authToken=deep\n";
    assert_eq!(
        auth("https://h.test/api/npm/", npmrc).as_deref(),
        Some("Bearer deep")
    );
    assert_eq!(
        auth("https://h.test/other/", npmrc).as_deref(),
        Some("Bearer root")
    );
    let only_deep = "//h.test/api/npm/:_authToken=deep\n";
    assert_eq!(auth("https://h.test/", only_deep), None);
    assert_eq!(auth("https://h.test/api/npmx/", only_deep), None);
}

#[test]
fn resolved_credentials_retain_the_winning_normalized_scope() {
    let base = Url::parse("https://h.test/api/npm/").unwrap();
    let authorization = authorization_for(
        &base,
        "//h.test/:_authToken=root\n//H.TEST/api/npm:_authToken=old\n//h.test/api/npm/:_auth=ZmFrZQ==\n",
        |_| None,
    ).expect("scoped credential");
    let registry = super::super::types::ResolvedNpmRegistry {
        base,
        authorization: Some(authorization),
    };
    assert_eq!(
        registry.authorization.as_ref().unwrap().scope,
        "//h.test/api/npm/"
    );
    for target in [
        "https://h.test/api/npm/pkg",
        "https://h.test:443/api/npm/pkg",
    ] {
        assert_eq!(
            registry
                .authorization_for(&Url::parse(target).unwrap())
                .unwrap()
                .expose_secret(),
            "Basic ZmFrZQ=="
        );
    }
    for target in [
        "https://h.test/other/pkg",
        "https://h.test/api/npm-extra/pkg",
        "https://h.test:444/api/npm/pkg",
        "http://h.test/api/npm/pkg",
        "https://fake:secret@h.test/api/npm/pkg",
    ] {
        assert!(
            registry
                .authorization_for(&Url::parse(target).unwrap())
                .is_none(),
            "{target}"
        );
    }
    octocode_runtime_base::test_support::checks::assert_absent(
        &serde_json::json!(format!("{registry:?}")),
        "ZmFrZQ==",
    );
}

#[test]
fn env_expansion_basic_auth_ports_and_comments() {
    let npmrc = "# c\n; c\nregistry=https://evil.example/\n\
                 //127.0.0.1:4873/:_authToken=${NPM_TOKEN}\n\
                 //basic.test/:_auth=\"dXNlcjpwYXNz\"\n\
                 //missing.test/:_authToken=${UNSET_VAR}\n";
    assert_eq!(
        auth("http://127.0.0.1:4873", npmrc).as_deref(),
        Some("Bearer from-env")
    );
    assert_eq!(
        auth("https://basic.test/", npmrc).as_deref(),
        Some("Basic dXNlcjpwYXNz")
    );
    assert_eq!(auth("https://missing.test/", npmrc), None);
    // A bare `registry=` line never attaches credentials anywhere.
    assert_eq!(auth("https://evil.example/", npmrc), None);
}
