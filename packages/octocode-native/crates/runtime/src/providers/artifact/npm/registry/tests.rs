use super::{test_registry, validate_registry, validate_resolved_addresses};
use crate::providers::artifact::query::artifact_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use std::net::SocketAddr;

fn npm_query(registry: Option<&str>) -> ArtifactSearchQuery {
    artifact_query(
        serde_json::json!({"operation":"resolve","ecosystem": ArtifactType::Npm, "packageName": "left-pad".to_string(), "registryUrl": registry.map(str::to_owned)}),
        None,
    )
}

#[test]
fn rejects_link_local_metadata_registry() {
    let registry = test_registry("http://169.254.169.254");
    let query = npm_query(Some("http://169.254.169.254"));
    let err = validate_registry(&query, &registry, false).expect_err("must reject link-local host");
    // The opt-in escape hatch permits the same host.
    assert!(validate_registry(&query, &registry, true).is_ok());
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

/// A custom registry is held to the shared outbound-address policy
/// ([`crate::policy::network::is_public`]): an address that is internal,
/// reserved, or tunnels one is refused, as a literal host or a DNS answer.
#[test]
fn a_registry_must_resolve_to_a_public_address() {
    for internal in [
        "127.0.0.1",
        "10.0.0.5",
        "169.254.169.254",
        "100.64.0.1",
        "224.0.0.1",
        "192.0.2.1",
        "[::1]",
        "[fc00::1]",
        "[64:ff9b::a00:1]",
        "[2002:c0a8:101::1]",
    ] {
        let url = format!("https://{internal}/");
        let error =
            validate_registry(&npm_query(Some(&url)), &test_registry(&url), false).expect_err(&url);
        assert_eq!(error.code, ErrorCode::InvalidInput, "{url}");
        let ip = internal.trim_matches(['[', ']']).parse().expect("address");
        assert!(validate_resolved_addresses(&[SocketAddr::new(ip, 443)]).is_err());
    }
    let public = "https://104.16.0.1/";
    assert!(validate_registry(&npm_query(Some(public)), &test_registry(public), false).is_ok());
}

/// `localhost` names are refused by name, before DNS: a resolver may map
/// a `*.localhost` name anywhere (RFC 6761 only says it should not).
#[test]
fn a_registry_named_localhost_is_refused_without_resolving_it() {
    for url in [
        "http://localhost:4873/",
        "https://LOCALHOST./",
        "http://verdaccio.localhost/",
        "https://a.localhost./",
    ] {
        let error =
            validate_registry(&npm_query(Some(url)), &test_registry(url), false).expect_err(url);
        assert_eq!(error.code, ErrorCode::InvalidInput, "{url}");
        assert!(validate_registry(&npm_query(Some(url)), &test_registry(url), true).is_ok());
    }
}

#[test]
fn rejects_mixed_public_private_dns_answers() {
    let addresses = [
        SocketAddr::from(([104, 16, 0, 1], 443)),
        SocketAddr::from(([127, 0, 0, 1], 443)),
    ];
    let error = validate_resolved_addresses(&addresses)
        .expect_err("one blocked answer must reject the complete DNS result");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(validate_resolved_addresses(&[addresses[0]]).is_ok());
}
