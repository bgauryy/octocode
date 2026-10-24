use super::registry::test_registry;
use super::{
    ArtifactError, ArtifactProviderPage, NPM_INSTALL_JSON, RegistryClient, ResolvedNpmRegistry,
};
use crate::providers::artifact::ArtifactCache;
use crate::providers::artifact::query::artifact_query;
use crate::providers::artifact::test_http::test_budget;
use crate::providers::artifact::test_http::{KnownTags, RouteHttp};
use base64::Engine as _;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

mod provenance;

const JSON: &str = "application/json";
const INSTALL: &str = NPM_INSTALL_JSON;
const COMMIT: &str = "59bbc03e10c636b9eb3c393dfeb552819774ec21";
const TARBALL: &[u8] = b"zod tarball bytes";

/// A registry at `url` whose authorization comes from `npmrc` text.
fn npmrc_registry(url: &str, npmrc: &str) -> ResolvedNpmRegistry {
    let mut registry = test_registry(url);
    registry.authorization =
        crate::providers::artifact::npmrc::authorization_for(&registry.base, npmrc, |_| None);
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

async fn lookup(http: &RouteHttp, fields: Value) -> Result<ArtifactProviderPage, ArtifactError> {
    let budget = test_budget();
    let client = RegistryClient::uncached(http, &budget);
    let mut query = json!({"operation":"search","ecosystem": "npm"});
    for (key, value) in fields.as_object().expect("fields") {
        query[key] = value.clone();
    }
    let query = artifact_query(query, None);
    super::npm(
        &query,
        &Default::default(),
        &test_registry("https://registry.npmjs.org/"),
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

#[tokio::test]
async fn npm_version_without_peers_has_no_peer_list() {
    let mut bare = manifest("4.6.5", false);
    bare["peerDependencies"] = json!({});
    let http = routes(vec![("/zod/latest", JSON, bare)]);
    let page = lookup(&http, json!({"operation":"resolve","packageName": "zod"}))
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.dependencies, Some(Vec::new()));
    assert_eq!(item.peer_dependencies, None);
}

#[tokio::test]
async fn npm_versions_resolve_exact_range_and_tag() {
    let http = routes(vec![("/zod/latest", JSON, manifest("4.6.5", false))]);
    let range = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "^3"}),
    )
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
    let coordinate = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "3.22.0"}),
    )
    .await
    .expect("coordinate");
    assert_eq!(coordinate.artifacts[0].version.as_deref(), Some("3.22.0"));
    assert_eq!(
        coordinate.artifacts[0].deprecated.as_deref(),
        Some("Use 3.22.4")
    );
    let tag = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "latest"}),
    )
    .await
    .expect("tag");
    assert_eq!(tag.artifacts[0].version.as_deref(), Some("4.6.5"));
    let missing = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "3.22.9"}),
    )
    .await
    .expect_err("missing version");
    assert_eq!(missing.code, ErrorCode::VersionNotFound);
    assert_eq!(missing.status, Some(404));
    assert!(
        missing.nearest.iter().any(|version| version == "3.22.0"),
        "{missing:?}"
    );
    let unmatched = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "^9"}),
    )
    .await
    .expect_err("no match");
    assert_eq!(unmatched.code, ErrorCode::VersionNotFound);
    let beyond = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "99.0.0"}),
    )
    .await
    .expect_err("beyond every release");
    assert_eq!(beyond.code, ErrorCode::VersionNotFound);
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
    let tags = KnownTags::new(vec!["colinhacks/zod@v3.25.76", "colinhacks/zod@aaaaaaa1"]);
    let budget = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::uncached(&http, &budget)
    };
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": "npm", "packageName": "zod", "version": "3.25.76"}),
        None,
    );
    let registry = test_registry("https://registry.npmjs.org/");
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
    let tag_only = KnownTags::new(vec!["colinhacks/zod@v3.25.76"]);
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
    let cache = ArtifactCache::new(None);
    let budget = test_budget();
    let client = RegistryClient {
        cache: Some(&cache),
        ..RegistryClient::uncached(&http, &budget)
    };
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": "npm", "packageName": "zod", "version": "^3"}),
        None,
    );
    let registry = test_registry("https://registry.npmjs.org/");
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
    let page = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "3.22.0"}),
    )
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
    let page = lookup(
        &http,
        json!({"operation":"resolve","packageName": "zod", "version": "3.22.0"}),
    )
    .await
    .expect("exact");
    assert_eq!(page.artifacts[0].optional_dependencies, None);
}
