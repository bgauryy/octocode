use super::super::types::artifact_query;
use super::{
    ArtifactSearchQuery, ArtifactType, ResolvedNpmRegistry, validate_registry,
    validate_resolved_addresses,
};
use std::net::SocketAddr;
use url::Url;

fn npm_registry(url: &str) -> ResolvedNpmRegistry {
    ResolvedNpmRegistry {
        base: Url::parse(url).expect("valid url"),
        authorization: None,
        cache_identity: "test".into(),
    }
}

fn npm_query(registry: Option<&str>) -> ArtifactSearchQuery {
    artifact_query(
        serde_json::json!({"ecosystem": ArtifactType::Npm, "packageName": "left-pad".to_string(), "registryUrl": registry.map(str::to_owned)}),
        None,
    )
}

#[test]
fn rejects_link_local_metadata_registry() {
    let registry = npm_registry("http://169.254.169.254");
    let query = npm_query(Some("http://169.254.169.254"));
    let err = validate_registry(&query, &registry, false).expect_err("must reject link-local host");
    // The opt-in escape hatch permits the same host.
    assert!(validate_registry(&query, &registry, true).is_ok());
    assert_eq!(err.code, "invalidInput");
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
            validate_registry(&npm_query(Some(&url)), &npm_registry(&url), false).expect_err(&url);
        assert_eq!(error.code, "invalidInput", "{url}");
        let ip = internal.trim_matches(['[', ']']).parse().expect("address");
        assert!(validate_resolved_addresses(&[SocketAddr::new(ip, 443)]).is_err());
    }
    let public = "https://104.16.0.1/";
    assert!(validate_registry(&npm_query(Some(public)), &npm_registry(public), false).is_ok());
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
            validate_registry(&npm_query(Some(url)), &npm_registry(url), false).expect_err(url);
        assert_eq!(error.code, "invalidInput", "{url}");
        assert!(validate_registry(&npm_query(Some(url)), &npm_registry(url), true).is_ok());
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
    assert_eq!(error.code, "invalidInput");
    assert!(validate_resolved_addresses(&[addresses[0]]).is_ok());
}

use super::super::http::RegistryClient;
use super::super::test_http::RouteHttp;
use base64::Engine as _;
use serde_json::{Value, json};

const JSON: &str = "application/json";
const INSTALL: &str = super::NPM_INSTALL_JSON;
const COMMIT: &str = "59bbc03e10c636b9eb3c393dfeb552819774ec21";
const TARBALL: &[u8] = b"zod tarball bytes";

/// A registry at `url` whose authorization comes from `npmrc` text.
fn npmrc_registry(url: &str, npmrc: &str) -> ResolvedNpmRegistry {
    let mut registry = npm_registry(url);
    registry.authorization =
        super::super::npmrc::authorization_for(&registry.base, npmrc, |_| None);
    registry
}

fn sha512(bytes: &[u8]) -> Vec<u8> {
    use sha2::Digest as _;
    sha2::Sha512::digest(bytes).to_vec()
}

fn manifest(version: &str, attested: bool) -> Value {
    let integrity = format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(sha512(TARBALL))
    );
    let mut dist = json!({"integrity": integrity});
    if attested {
        dist["attestations"] = json!({
            "url": format!("https://registry.npmjs.org/-/npm/v1/attestations/zod@{version}"),
            "provenance": {"predicateType": "https://slsa.dev/provenance/v1"}
        });
    }
    json!({
        "name": "zod", "version": version, "license": "MIT",
        "description": "schemas", "homepage": "https://github.com/colinhacks/zod#readme",
        "repository": {"type": "git", "url": "git+https://github.com/colinhacks/zod.git"},
        "gitHead": "aaaaaaa1",
        "dependencies": {}, "peerDependencies": {"typescript": "*"},
        "engines": {"node": ">=18", "npm": ">=9", "pnpm": ">=8"},
        "deprecated": (version == "3.22.0").then_some("Use 3.22.4"),
        "_npmOperationalInternal": {"tmp": format!("tmp/zod_{version}_1789341914484_0.59")},
        "dist": dist
    })
}

fn bundle(version: &str, repo: &str, digest: &[u8]) -> Value {
    let statement = json!({
        "subject": [{"name": format!("pkg:npm/zod@{version}"), "digest": {"sha512": hex::encode(digest)}}],
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {"buildDefinition": {"resolvedDependencies": [
            {"uri": format!("git+{repo}@refs/heads/main"), "digest": {"gitCommit": COMMIT}}
        ]}}
    });
    let payload = base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&statement).expect("statement"));
    json!({"attestations": [
        {"predicateType": "https://github.com/npm/attestation/tree/main/specs/publish/v0.1", "bundle": {}},
        {"predicateType": "https://slsa.dev/provenance/v1", "bundle": {"dsseEnvelope": {"payload": payload}}}
    ]})
}

async fn lookup(
    http: &RouteHttp,
    fields: Value,
) -> Result<super::ArtifactProviderPage, super::ArtifactError> {
    let budget = super::super::types::test_budget();
    let client = RegistryClient::uncached(http, &budget);
    let mut query = json!({"ecosystem": "npm"});
    for (key, value) in fields.as_object().expect("fields") {
        query[key] = value.clone();
    }
    let query = artifact_query(query, None);
    super::npm(
        &query,
        &Default::default(),
        &npm_registry("https://registry.npmjs.org/"),
        &client,
        false,
    )
    .await
}

fn routes(extra: Vec<(&'static str, &'static str, Value)>) -> RouteHttp {
    let mut routes = vec![
        (
            "/zod",
            INSTALL,
            json!({"name":"zod","dist-tags":{"latest":"4.6.5"},
            "versions":{"3.22.0":{},"3.25.76":{},"4.6.5":{}}}),
        ),
        ("/zod/3.25.76", JSON, manifest("3.25.76", false)),
        ("/zod/3.22.0", JSON, manifest("3.22.0", false)),
    ];
    routes.extend(extra);
    routes
        .into_iter()
        .fold(RouteHttp::new(), |http, (path, accept, body)| {
            http.json_for(path, accept, body)
        })
}

/// Each request's URL and whether it carried credentials.
fn requests(http: &RouteHttp) -> Vec<(String, bool)> {
    http.seen()
        .iter()
        .map(|seen| (seen.url.to_string(), seen.authorized))
        .collect()
}

/// A version that declares no peers shows no `peerDependencies` list;
/// its (empty) `dependencies` list still says the manifest was read.
#[tokio::test]
async fn npm_version_without_peers_has_no_peer_list() {
    let mut bare = manifest("4.6.5", false);
    bare["peerDependencies"] = json!({});
    let http = routes(vec![("/zod/latest", JSON, bare)]);
    let page = lookup(&http, json!({"packageName": "zod"}))
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.dependencies, Some(Vec::new()));
    assert_eq!(item.peer_dependencies, None);
}

#[tokio::test]
async fn npm_versions_resolve_exact_range_tag_and_coordinate() {
    let http = routes(vec![("/zod/latest", JSON, manifest("4.6.5", false))]);
    let range = lookup(&http, json!({"packageName": "zod", "version": "^3"}))
        .await
        .expect("range");
    let item = &range.artifacts[0];
    assert_eq!(item.version.as_deref(), Some("3.25.76"));
    assert_eq!(item.published_at.as_deref(), Some("2026-09-13"));
    assert_eq!(item.dependencies, Some(Vec::new()));
    assert_eq!(
        item.peer_dependencies.as_ref().map(Vec::len),
        Some(1),
        "{:?}",
        item.peer_dependencies
    );
    // Every declared engine stays: not only `node`.
    assert_eq!(
        item.engines,
        Some(std::collections::BTreeMap::from([
            ("node".to_owned(), ">=18".to_owned()),
            ("npm".to_owned(), ">=9".to_owned()),
            ("pnpm".to_owned(), ">=8".to_owned()),
        ]))
    );
    let coordinate = lookup(&http, json!({"packageName": "zod@3.22.0"}))
        .await
        .expect("coordinate");
    assert_eq!(coordinate.artifacts[0].version.as_deref(), Some("3.22.0"));
    assert_eq!(
        coordinate.artifacts[0].deprecated.as_deref(),
        Some("Use 3.22.4")
    );
    let tag = lookup(&http, json!({"packageName": "zod", "version": "latest"}))
        .await
        .expect("tag");
    assert_eq!(tag.artifacts[0].version.as_deref(), Some("4.6.5"));
    let missing = lookup(&http, json!({"packageName": "zod", "version": "3.22.9"}))
        .await
        .expect_err("missing version");
    assert_eq!(missing.code, "versionNotFound");
    assert_eq!(missing.status, Some(404));
    assert!(
        missing.nearest.iter().any(|version| version == "3.22.0"),
        "{missing:?}"
    );
    let unmatched = lookup(&http, json!({"packageName": "zod", "version": "^9"}))
        .await
        .expect_err("no match");
    assert_eq!(unmatched.code, "versionNotFound");
    let beyond = lookup(&http, json!({"packageName": "zod", "version": "99.0.0"}))
        .await
        .expect_err("beyond every release");
    assert_eq!(beyond.code, "versionNotFound");
    assert_eq!(beyond.nearest, ["4.6.5", "3.25.76", "3.22.0"], "{beyond:?}");
    let seen = http
        .seen()
        .iter()
        .map(|seen| format!("{} {}", seen.accept, seen.url.path()))
        .collect::<Vec<_>>();
    assert_eq!(
        seen[..2],
        [format!("{INSTALL} /zod"), format!("{JSON} /zod/3.25.76")],
        "a range reads the abbreviated packument, then one manifest"
    );
}

/// A manifest without `gitHead` or provenance pins its source to the
/// upstream release tag, the fallback every registry that names no
/// commit takes; a recorded `gitHead` stays the ref without a tag check.
#[tokio::test]
async fn npm_without_a_recorded_commit_pins_the_upstream_release_tag() {
    let mut headless = manifest("3.25.76", false);
    headless
        .as_object_mut()
        .expect("manifest")
        .remove("gitHead");
    let http = routes(vec![("/zod/3.25.76", JSON, headless)]);
    let tags = super::super::test_http::KnownTags::new(vec![
        "colinhacks/zod@v3.25.76",
        "colinhacks/zod@aaaaaaa1",
    ]);
    let budget = super::super::types::test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::uncached(&http, &budget)
    };
    let query = artifact_query(
        json!({"ecosystem": "npm", "packageName": "zod", "version": "3.25.76"}),
        None,
    );
    let registry = npm_registry("https://registry.npmjs.org/");
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some("v3.25.76"));
    assert!(item.source_tag);
    let with_head = routes(Vec::new());
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::uncached(&with_head, &budget)
    };
    let checks = tags.seen().len();
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    assert_eq!(page.artifacts[0].source_ref.as_deref(), Some("aaaaaaa1"));
    assert!(!page.artifacts[0].source_tag);
    assert_eq!(
        tags.seen()[checks..],
        ["colinhacks/zod@aaaaaaa1"],
        "only the gitHead is checked"
    );
    // A gitHead GitHub lacks is named, and the release tag pins instead.
    let tag_only = super::super::test_http::KnownTags::new(vec!["colinhacks/zod@v3.25.76"]);
    let client = RegistryClient {
        tags: Some(&tag_only),
        ..RegistryClient::uncached(&with_head, &budget)
    };
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.missing_ref.as_deref(), Some("aaaaaaa1"));
    assert_eq!(item.source_ref.as_deref(), Some("v3.25.76"));
}

/// A range reads the abbreviated packument for its version list only:
/// that view (registry order, `latest`) is what is decoded and cached,
/// so a repeat resolves without the registry.
#[tokio::test]
async fn a_range_caches_only_the_version_view_of_the_packument() {
    let view = super::version_view(
        br#"{"name":"zod","dist-tags":{"latest":"4.6.5","next":"5.0.0-beta"},
        "versions":{"3.22.0":{"dist":{"x":[1,2]}},"10.0.0":{},"4.6.5":{}},"modified":"x"}"#,
    )
    .expect("view");
    assert_eq!(
        view,
        json!({"versions": ["3.22.0", "10.0.0", "4.6.5"], "latest": "4.6.5"})
    );
    let http = routes(vec![("/zod/latest", JSON, manifest("4.6.5", false))]);
    let cache = super::super::ArtifactCache::new(None);
    let budget = super::super::types::test_budget();
    let client = RegistryClient {
        cache: Some(&cache),
        ..RegistryClient::uncached(&http, &budget)
    };
    let query = artifact_query(
        json!({"ecosystem": "npm", "packageName": "zod", "version": "^3"}),
        None,
    );
    let registry = npm_registry("https://registry.npmjs.org/");
    for _ in 0..2 {
        let page = super::npm(&query, &Default::default(), &registry, &client, false)
            .await
            .expect("range");
        assert_eq!(page.artifacts[0].version.as_deref(), Some("3.25.76"));
    }
    assert_eq!(http.seen().len(), 2, "packument and manifest once each");
}

/// `dependencies` lists the required ones as `name@range`: npm repeats
/// each optional dependency under `dependencies`, and those are listed
/// in `optionalDependencies` instead; a version without optional ones
/// shows no such list.
#[tokio::test]
async fn npm_optional_dependencies_are_listed_apart_from_required_ones() {
    let mut row = manifest("3.22.0", false);
    row["dependencies"] = json!({"a": "^1.0.0", "@next/swc-darwin-arm64": "15.0.0"});
    row["optionalDependencies"] = json!({"@next/swc-darwin-arm64": "15.0.0"});
    let http = routes(vec![]).json_for("/zod/3.22.0", JSON, row);
    let page = lookup(&http, json!({"packageName": "zod", "version": "3.22.0"}))
        .await
        .expect("exact");
    assert_eq!(
        page.artifacts[0].dependencies,
        Some(vec!["a@^1.0.0".to_owned()])
    );
    assert_eq!(
        page.artifacts[0].optional_dependencies,
        Some(vec!["@next/swc-darwin-arm64@15.0.0".to_owned()])
    );
    let mut bare = manifest("3.22.0", false);
    bare["dependencies"] = json!({"a": "^1.0.0"});
    let http = routes(vec![]).json_for("/zod/3.22.0", JSON, bare);
    let page = lookup(&http, json!({"packageName": "zod", "version": "3.22.0"}))
        .await
        .expect("exact");
    assert_eq!(page.artifacts[0].optional_dependencies, None);
}

#[tokio::test]
async fn npm_provenance_pins_the_attested_commit_only_when_bound() {
    let good = routes(vec![
        ("/zod/4.6.5", JSON, manifest("4.6.5", true)),
        (
            "/-/npm/v1/attestations/zod@4.6.5",
            JSON,
            bundle(
                "4.6.5",
                "https://github.com/colinhacks/zod",
                &sha512(TARBALL),
            ),
        ),
    ]);
    let page = lookup(&good, json!({"packageName": "zod", "version": "4.6.5"}))
        .await
        .expect("attested");
    let item = &page.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some(COMMIT));
    assert!(item.source_attested);

    for (repo, digest) in [
        ("https://github.com/evil/zod", sha512(TARBALL)),
        (
            "https://github.com/colinhacks/zod",
            sha512(b"other tarball"),
        ),
    ] {
        let spoofed = routes(vec![
            ("/zod/4.6.5", JSON, manifest("4.6.5", true)),
            (
                "/-/npm/v1/attestations/zod@4.6.5",
                JSON,
                bundle("4.6.5", repo, &digest),
            ),
        ]);
        let page = lookup(&spoofed, json!({"packageName": "zod", "version": "4.6.5"}))
            .await
            .expect("unbound attestation");
        let item = &page.artifacts[0];
        assert!(!item.source_attested, "{repo}");
        assert_eq!(
            item.source_ref.as_deref(),
            Some("aaaaaaa1"),
            "gitHead lead stays"
        );
    }
}

#[tokio::test]
async fn attestation_requests_keep_fake_credentials_on_the_registry_origin() {
    for (url, allowed) in [
        ("https://registry.npmjs.org:444/attestation", false),
        ("https://fake:secret@registry.npmjs.org/attestation", false),
        ("https://registry.npmjs.org:443/attestation", true),
    ] {
        let http = routes(vec![(
            "/attestation",
            JSON,
            bundle(
                "4.6.5",
                "https://github.com/colinhacks/zod",
                &sha512(TARBALL),
            ),
        )]);
        let budget = super::super::types::test_budget();
        let client = RegistryClient::uncached(&http, &budget);
        let registry = npmrc_registry(
            "https://registry.npmjs.org/",
            "//registry.npmjs.org/:_authToken=fake-test-token",
        );
        let mut item =
            super::ArtifactItem::new(ArtifactType::Npm, "zod".into(), registry.base.to_string());
        item.repository = Some("https://github.com/colinhacks/zod".into());
        let mut row = manifest("4.6.5", true);
        row["dist"]["attestations"]["url"] = json!(url);
        let commit = super::attested_commit(
            &item,
            "4.6.5",
            row.as_object().expect("manifest"),
            &registry,
            &client,
            None,
        )
        .await;
        let requests = requests(&http);
        if allowed {
            assert_eq!(commit.as_deref(), Some(COMMIT));
            assert_eq!(requests.len(), 1);
            assert_eq!(
                requests[0],
                ("https://registry.npmjs.org/attestation".into(), true)
            );
        } else {
            assert!(
                requests.is_empty(),
                "off-origin or userinfo URL reached authenticated transport: {url}"
            );
            assert!(commit.is_none());
        }
    }
}

#[tokio::test]
async fn npm_path_scoped_credentials_do_not_reach_sibling_attestations() {
    for (scope, target, authenticated) in [
        ("/api/npm/", "/api/npm/attestation", true),
        ("/api/npm/", "/other-service/attestation", false),
        ("/api/npm/", "/api/npm-extra/attestation", false),
        ("/", "/other-service/attestation", true),
    ] {
        let http = routes(vec![(
            target,
            JSON,
            bundle(
                "4.6.5",
                "https://github.com/colinhacks/zod",
                &sha512(TARBALL),
            ),
        )]);
        let budget = super::super::types::test_budget();
        let client = RegistryClient::uncached(&http, &budget);
        let registry = npmrc_registry(
            "https://registry.npmjs.org/api/npm/",
            &format!("//registry.npmjs.org{scope}:_authToken=fake-path-test-token"),
        );
        assert!(registry.authorization.is_some());
        let mut item =
            super::ArtifactItem::new(ArtifactType::Npm, "zod".into(), registry.base.to_string());
        item.repository = Some("https://github.com/colinhacks/zod".into());
        let mut row = manifest("4.6.5", true);
        let url = format!("https://registry.npmjs.org{target}");
        row["dist"]["attestations"]["url"] = json!(url);
        let commit = super::attested_commit(
            &item,
            "4.6.5",
            row.as_object().expect("manifest"),
            &registry,
            &client,
            None,
        )
        .await;
        assert_eq!(
            commit.as_deref(),
            Some(COMMIT),
            "anonymous optional attestations remain usable"
        );
        assert_eq!(
            requests(&http),
            [(url, authenticated)],
            "credential scope {scope}, target {target}"
        );
    }
}

#[tokio::test]
async fn npm_path_scoped_credentials_authorize_metadata_requests() {
    let http = routes(vec![
        (
            "/api/npm/zod",
            INSTALL,
            json!({"name":"zod","dist-tags":{"latest":"4.6.5"},"versions":{"4.6.5":{}}}),
        ),
        ("/api/npm/zod/4.6.5", JSON, manifest("4.6.5", false)),
        (
            "/api/npm/-/v1/search",
            JSON,
            json!({"objects":[],"total":0}),
        ),
    ]);
    let budget = super::super::types::test_budget();
    let client = RegistryClient::uncached(&http, &budget);
    let registry = npmrc_registry(
        "https://registry.npmjs.org/api/npm/",
        "//registry.npmjs.org/api/npm/:_authToken=fake-metadata-token",
    );
    super::exact("zod", Some("^4"), &registry, &client, None)
        .await
        .expect("packument and manifest");
    let query = artifact_query(json!({"ecosystem":"npm","keywords":["schema"]}), None);
    super::search(&query, &Default::default(), &registry, &client, None)
        .await
        .expect("discovery");
    let requests = requests(&http);
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|(url, auth)| url.starts_with("https://registry.npmjs.org/api/npm/") && *auth),
        "{requests:?}"
    );
}

#[test]
fn splits_versioned_and_scoped_coordinates() {
    assert_eq!(
        super::split_npm_coordinate("leftpad@1.2.3"),
        ("leftpad", Some("1.2.3"))
    );
    assert_eq!(
        super::split_npm_coordinate("@scope/pkg@9.0.0"),
        ("@scope/pkg", Some("9.0.0"))
    );
    assert_eq!(
        super::split_npm_coordinate("@scope/pkg"),
        ("@scope/pkg", None)
    );
}
