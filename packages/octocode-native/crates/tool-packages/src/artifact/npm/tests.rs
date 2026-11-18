use super::registry::test_registry;
use super::{
    ArtifactError, ArtifactProviderPage, NPM_INSTALL_JSON, OPTIONAL_PEER, RegistryClient,
    ResolvedNpmRegistry,
};
use crate::artifact::query::artifact_query;
use crate::artifact::test_http::test_budget;
use crate::artifact::test_http::{KnownTags, RouteHttp};
use base64::Engine as _;
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

mod metadata;

mod provenance;

const JSON: &str = "application/json";
const INSTALL: &str = NPM_INSTALL_JSON;
const COMMIT: &str = "59bbc03e10c636b9eb3c393dfeb552819774ec21";
const TARBALL: &[u8] = b"zod tarball bytes";

/// A registry at `url` whose authorization comes from `npmrc` text.
fn npmrc_registry(url: &str, npmrc: &str) -> ResolvedNpmRegistry {
    let mut registry = test_registry(url);
    registry.authorization =
        crate::artifact::npmrc::authorization_for(&registry.base, npmrc, |_| None);
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
    let client = RegistryClient::new(http, &budget);
    let mut query = json_fixture(r#"{"operation": "search", "ecosystem": "npm"}"#);
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
            json_fixture(
                r#"{
                    "name": "zod",
                    "dist-tags": {"latest": "4.6.5"},
                    "versions": {"3.22.0": {}, "3.25.76": {}, "4.6.5": {}}
                }"#,
            ),
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
    let page = lookup(
        &http,
        json_fixture(r#"{"operation": "resolve", "packageName": "zod"}"#),
    )
    .await
    .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.dependencies, Some(Vec::new()));
    assert_eq!(item.peer_dependencies, None);
}

/// A peer `peerDependenciesMeta` marks optional is listed as optional; the
/// others, and a meta entry for a name that is no peer, stay as declared.
#[tokio::test]
async fn npm_optional_peers_are_marked() {
    let mut peers = manifest("4.6.5", false);
    peers["peerDependencies"] =
        json_fixture(r#"{"react": "^19", "react-dom": "^19", "vue": "^3"}"#);
    peers["peerDependenciesMeta"] = json_fixture(
        r#"{
            "react-dom": {"optional": true},
            "vue": {"optional": false},
            "not-a-peer": {"optional": true}
        }"#,
    );
    let http = routes(vec![("/zod/latest", JSON, peers)]);
    let page = lookup(
        &http,
        json_fixture(r#"{"operation": "resolve", "packageName": "zod"}"#),
    )
    .await
    .expect("lookup");
    assert_eq!(
        page.artifacts[0].peer_dependencies,
        Some(vec![
            "react@^19".to_owned(),
            format!("react-dom@^19{OPTIONAL_PEER}"),
            "vue@^3".to_owned(),
        ])
    );
}

#[tokio::test]
async fn npm_versions_resolve_exact_range_and_tag() {
    let http = routes(vec![("/zod/latest", JSON, manifest("4.6.5", false))]);
    let range = lookup(
        &http,
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "^3"
            }"#,
        ),
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
        Some(
            [("node", ">=18"), ("npm", ">=9"), ("pnpm", ">=8")]
                .map(
                    |(name, range)| octocode_contract_types::ArtifactSearchRowEnginesItem {
                        name: name.to_owned(),
                        range: range.to_owned(),
                    }
                )
                .to_vec()
        )
    );
    let coordinate = lookup(
        &http,
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "3.22.0"
            }"#,
        ),
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
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "latest"
            }"#,
        ),
    )
    .await
    .expect("tag");
    assert_eq!(tag.artifacts[0].version.as_deref(), Some("4.6.5"));
    let missing = lookup(
        &http,
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "3.22.9"
            }"#,
        ),
    )
    .await
    .expect_err("missing version");
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert_eq!(missing.status, Some(404));
    assert!(
        missing.nearest.iter().any(|version| version == "3.22.0"),
        "{missing:?}"
    );
    let unmatched = lookup(
        &http,
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "^9"
            }"#,
        ),
    )
    .await
    .expect_err("no match");
    assert_eq!(unmatched.code, ErrorCode::NotFound);
    let beyond = lookup(
        &http,
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "99.0.0"
            }"#,
        ),
    )
    .await
    .expect_err("beyond every release");
    assert_eq!(beyond.code, ErrorCode::NotFound);
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
        ..RegistryClient::new(&http, &budget)
    };
    let query = artifact_query(
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "zod",
                "version": "3.25.76"
            }"#,
        ),
        None,
    );
    let registry = test_registry("https://registry.npmjs.org/");
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some("commit-of-v3.25.76"));
    assert_eq!(item.release_tag.as_deref(), Some("v3.25.76"));
    assert!(item.source_tag);
    let with_head = routes(Vec::new());
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&with_head, &budget)
    };
    let checks = tags.seen().len();
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    assert_eq!(page.artifacts[0].source_ref.as_deref(), Some("aaaaaaa1"));
    assert!(!page.artifacts[0].source_tag);
    assert_eq!(
        page.artifacts[0]
            .tag_differs
            .as_ref()
            .map(|tag| tag.tag.as_str()),
        Some("v3.25.76"),
        "the release tag names another commit"
    );
    assert_eq!(
        tags.seen()[checks..],
        [
            "colinhacks/zod@aaaaaaa1",
            "colinhacks/zod@v3.25.76",
            "colinhacks/zod@3.25.76"
        ],
        "the gitHead and the plain release tags are checked"
    );
    // A gitHead GitHub lacks is named, and the release tag pins instead.
    let tag_only = KnownTags::new(vec!["colinhacks/zod@v3.25.76"]);
    let client = RegistryClient {
        tags: Some(&tag_only),
        ..RegistryClient::new(&with_head, &budget)
    };
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.missing_ref.as_deref(), Some("aaaaaaa1"));
    assert_eq!(item.source_ref.as_deref(), Some("commit-of-v3.25.76"));
    assert_eq!(item.release_tag.as_deref(), Some("v3.25.76"));
}

/// A recorded gitHead the release tag names is checked (`source_tag`), and no
/// tag at all leaves it as the registry's claim.
#[tokio::test]
async fn a_release_tag_naming_the_recorded_commit_proves_it() {
    let http = routes(Vec::new());
    let budget = test_budget();
    let query = artifact_query(
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "zod",
                "version": "3.25.76"
            }"#,
        ),
        None,
    );
    let registry = test_registry("https://registry.npmjs.org/");
    let tied = KnownTags::new(vec![
        "colinhacks/zod@aaaaaaa1",
        "colinhacks/zod@v3.25.76=AAAAAAA1",
    ]);
    let client = RegistryClient {
        tags: Some(&tied),
        ..RegistryClient::new(&http, &budget)
    };
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    assert!(page.artifacts[0].source_tag, "the tag names the commit");
    assert_eq!(page.artifacts[0].source_ref.as_deref(), Some("aaaaaaa1"));
    assert!(page.artifacts[0].tag_differs.is_none());
    let untagged = KnownTags::new(vec!["colinhacks/zod@aaaaaaa1"]);
    let client = RegistryClient {
        tags: Some(&untagged),
        ..RegistryClient::new(&http, &budget)
    };
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    assert!(!page.artifacts[0].source_tag);
    assert!(page.artifacts[0].tag_differs.is_none());
}

/// A range reads the abbreviated packument for its version list only:
/// that view retains registry order, the dist-tags and which versions are
/// deprecated, without building full manifests.
#[tokio::test]
async fn a_range_decodes_only_the_current_version_view_of_the_packument() {
    let view = super::version_view(
        br#"{"name":"zod","dist-tags":{"latest":"4.6.5","next":"5.0.0-beta"},
        "versions":{"3.22.0":{"dist":{"x":[1,2]},"deprecated":"use 4"},"10.0.0":{},"4.6.5":{}},"modified":"x"}"#,
    )
    .expect("view");
    assert_eq!(
        view,
        json_fixture(
            r#"{
                "versions": ["3.22.0", "10.0.0", "4.6.5"],
                "tags": {"latest": "4.6.5", "next": "5.0.0-beta"},
                "deprecated": ["3.22.0"]
            }"#
        )
    );
    let http = routes(vec![("/zod/latest", JSON, manifest("4.6.5", false))]);
    let budget = test_budget();
    let client = RegistryClient::new(&http, &budget);
    let query = artifact_query(
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "zod",
                "version": "^3"
            }"#,
        ),
        None,
    );
    let registry = test_registry("https://registry.npmjs.org/");
    for _ in 0..2 {
        let page = super::npm(&query, &Default::default(), &registry, &client, false)
            .await
            .expect("range");
        assert_eq!(page.artifacts[0].version.as_deref(), Some("3.25.76"));
    }
    assert_eq!(
        http.seen().len(),
        4,
        "each call reads its packument and manifest"
    );
}

/// A release whose repository GitHub no longer has names where the registry's
/// latest release says the project lives, when that is another repository.
#[tokio::test]
async fn a_missing_repository_names_the_successor_the_latest_release_declares() {
    use crate::artifact::{RepositoryStatus, release_ref::CommitCheck};
    let mut latest = manifest("4.6.5", false);
    latest["repository"] =
        json_fixture(r#"{"type": "git", "url": "git://github.com/zod-next/zod.git"}"#);
    let http = routes(vec![("/zod/latest", JSON, latest)]);
    let tags = KnownTags::new(Vec::new()).with_repository(CommitCheck::RepositoryMissing);
    let budget = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &budget)
    };
    let query = artifact_query(
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "zod",
                "version": "3.25.76"
            }"#,
        ),
        None,
    );
    let registry = test_registry("https://registry.npmjs.org/");
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert_eq!(item.repository_status, Some(RepositoryStatus::Missing));
    assert_eq!(item.successor_repository.as_deref(), Some("zod-next/zod"));
    // The latest release naming the same repository is no successor.
    let same = routes(vec![("/zod/latest", JSON, manifest("4.6.5", false))]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&same, &budget)
    };
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    assert_eq!(page.artifacts[0].successor_repository, None);
}

/// A release that records no `gitHead` is pinned by its release tag, and
/// with no tag the repository check of `HEAD` finds it gone: that path
/// names the latest release's repository too, as the `gitHead` path does.
#[tokio::test]
async fn a_missing_repository_found_by_the_release_tag_path_names_the_successor() {
    use crate::artifact::{RepositoryStatus, release_ref::CommitCheck};
    let mut release = manifest("3.25.76", false);
    release
        .as_object_mut()
        .expect("manifest object")
        .remove("gitHead");
    let mut latest = manifest("4.6.5", false);
    latest["repository"] =
        json_fixture(r#"{"type": "git", "url": "git://github.com/zod-next/zod.git"}"#);
    let http = routes(vec![
        ("/zod/3.25.76", JSON, release),
        ("/zod/latest", JSON, latest),
    ]);
    let tags = KnownTags::new(Vec::new()).with_repository(CommitCheck::RepositoryMissing);
    let budget = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &budget)
    };
    let query = artifact_query(
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "zod",
                "version": "3.25.76"
            }"#,
        ),
        None,
    );
    let registry = test_registry("https://registry.npmjs.org/");
    let page = super::npm(&query, &Default::default(), &registry, &client, false)
        .await
        .expect("lookup");
    let item = &page.artifacts[0];
    assert!(
        tags.seen().iter().any(|seen| seen == "colinhacks/zod@HEAD"),
        "the release-tag path checked the repository: {:?}",
        tags.seen()
    );
    assert_eq!(item.repository_status, Some(RepositoryStatus::Missing));
    assert_eq!(item.successor_repository.as_deref(), Some("zod-next/zod"));
}

/// ALL-TOOLS-C D7: an older version's manifest carries no upload record
/// (`_npmOperationalInternal`), so its publish date comes from the full
/// packument's `time[<version>]`, which npm keeps for every version. A
/// manifest that dates itself reads nothing more.
#[tokio::test]
async fn an_old_version_is_dated_from_the_packument_time() {
    let mut old = manifest("1.2.0", false);
    old.as_object_mut()
        .expect("manifest")
        .remove("_npmOperationalInternal");
    let http = routes(vec![
        ("/zod/1.2.0", JSON, old),
        (
            "/zod",
            JSON,
            json_fixture(
                r#"{
                    "name": "zod",
                    "versions": {"1.2.0": {"name": "zod"}},
                    "time": {
                        "created": "2015-01-01T00:00:00.000Z",
                        "1.2.0": "2015-08-24T13:56:27.000Z"
                    }
                }"#,
            ),
        ),
    ]);
    let page = lookup(
        &http,
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "1.2.0"
            }"#,
        ),
    )
    .await
    .expect("lookup");
    assert_eq!(
        page.artifacts[0].published_at.as_deref(),
        Some("2015-08-24")
    );
    let dated = routes(vec![]);
    let page = lookup(
        &dated,
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "zod",
                "version": "3.22.0"
            }"#,
        ),
    )
    .await
    .expect("lookup");
    assert_eq!(
        page.artifacts[0].published_at.as_deref(),
        Some("2026-09-13")
    );
    assert!(
        !dated
            .seen()
            .iter()
            .any(|seen| seen.accept == JSON && seen.url.path() == "/zod"),
        "a manifest that dates itself reads no packument"
    );
}

/// B222: a full packument that cannot be read leaves an old version undated,
/// and the row says so, since a missing `publishedAt` is then no evidence
/// that npm records no date. A cancelled read fails the lookup.
#[tokio::test]
async fn an_unread_packument_date_is_disclosed_on_the_row() {
    let mut old = manifest("1.2.0", false);
    old.as_object_mut()
        .expect("manifest")
        .remove("_npmOperationalInternal");
    let resolve = json_fixture(
        r#"{
            "operation": "resolve",
            "packageName": "zod",
            "version": "1.2.0"
        }"#,
    );
    let limited = routes(vec![("/zod/1.2.0", JSON, old.clone())]).status("/zod", 429, json!({}));
    let page = lookup(&limited, resolve.clone())
        .await
        .expect("a rate-limited date read keeps the row");
    let item = &page.artifacts[0];
    assert_eq!(item.published_at, None);
    assert!(
        item.disclosures
            .iter()
            .any(|said| said.contains("zod 1.2.0")
                && said.contains("publish date")
                && said.contains("packument")),
        "{:?}",
        item.disclosures
    );
    let cancelled = routes(vec![("/zod/1.2.0", JSON, old)]).fails("/zod", ErrorCode::Cancelled);
    let error = lookup(&cancelled, resolve)
        .await
        .expect_err("a cancelled date read fails the lookup");
    assert_eq!(error.code, ErrorCode::Cancelled);
}
