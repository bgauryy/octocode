//! Go exact lookups: source refs, module directories, package paths, pins and vanity modules.
use super::super::query::{artifact_query, exact_query};
use super::super::test_http::RouteHttp;
use super::super::test_http::test_budget;
use super::*;

use octocode_test_support::json_fixture;
use serde_json::json;

mod retraction;

#[test]
fn go_source_ref_names_the_tag_or_pseudo_version_commit() {
    let item = |module: &str, version: &str| {
        let mut item = ArtifactItem::new(ArtifactType::Go, module.into(), String::new());
        item.module_path = Some(module.into());
        item.version = Some(version.into());
        item.repository = Some("https://github.com/o/r".into());
        go_source_ref(&item)
    };
    assert_eq!(
        item("github.com/o/r", "v1.10.2").as_deref(),
        Some("v1.10.2")
    );
    assert_eq!(
        item("github.com/o/r/v2", "v2.3.0").as_deref(),
        Some("v2.3.0")
    );
    assert_eq!(
        item("github.com/o/r/sub/mod", "v0.4.1").as_deref(),
        Some("sub/mod/v0.4.1")
    );
    assert_eq!(
        item("github.com/o/r", "v2.0.0+incompatible").as_deref(),
        Some("v2.0.0")
    );
    assert_eq!(
        item("github.com/o/r", "v0.0.0-20240101120000-abcdef123456").as_deref(),
        Some("abcdef123456")
    );
    assert_eq!(item("github.com/o/rx", "v1.0.0"), None);
}

/// A module inside a repository keeps its tag prefix, so the release lead
/// lists the module and not the repository root. A major-version suffix is
/// never part of the prefix: whether a `/vN` module's files sit in `vN` is
/// read from the repository's tree at its ref.
#[test]
fn go_module_directory_is_the_tag_prefix_under_the_repository_root() {
    let directory = |module: &str| {
        let mut item = ArtifactItem::new(ArtifactType::Go, module.into(), String::new());
        item.module_path = Some(module.into());
        item.repository = Some("https://github.com/aws/aws-sdk-go-v2".into());
        go_module_directory(&item)
    };
    assert_eq!(
        directory("github.com/aws/aws-sdk-go-v2/service/s3").as_deref(),
        Some("service/s3")
    );
    assert_eq!(
        directory("github.com/aws/aws-sdk-go-v2/v2").as_deref(),
        Some("")
    );
    assert_eq!(directory("go.example.com/x"), None);
}

/// An exact Go module row: its release day from the module record, and
/// its direct requirements from the version's `go.mod` on the module
/// proxy (`// indirect` ones left out); without that file the
/// dependencies are unknown, not none.
#[tokio::test]
async fn go_parses_exact_module_lookup() {
    let module = json_fixture(
        r#"{
            "path": "github.com/gin-gonic/gin",
            "version": "v1.9.1",
            "synopsis": "HTTP web framework for Go",
            "commitTime": "2023-06-12T08:50:28Z"
        }"#,
    );
    let go_mod = "module github.com/gin-gonic/gin\n\ngo 1.20\n\nrequire github.com/bytedance/sonic v1.9.1\n\
            require (\n\tgithub.com/gin-contrib/sse v0.1.0 // pinned\n\tgolang.org/x/net v0.10.0 // indirect\n)\n\
            replace example.com/x => ../x\n";
    let b = test_budget();
    let q = exact_query(ArtifactType::Go, "github.com/gin-gonic/gin");
    let http = RouteHttp::routes(vec![(
        "/v1/module/github.com/gin-gonic/gin",
        module.clone(),
    )])
    .bytes("/github.com/gin-gonic/gin/@v/v1.9.1.mod", go_mod);
    let client = RegistryClient::new(&http, &b);
    let page = go(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("go");
    assert_eq!(page.artifacts.len(), 1);
    let item = &page.artifacts[0];
    assert_eq!(item.package_name, "github.com/gin-gonic/gin");
    assert_eq!(item.version.as_deref(), Some("v1.9.1"));
    assert_eq!(item.published_at.as_deref(), Some("2023-06-12"));
    assert_eq!(
        item.registry_url,
        "https://pkg.go.dev/github.com/gin-gonic/gin"
    );
    assert_eq!(
        item.dependencies,
        Some(vec![
            "github.com/bytedance/sonic v1.9.1".to_owned(),
            "github.com/gin-contrib/sse v0.1.0".to_owned(),
        ])
    );
    let http = RouteHttp::routes(vec![("/v1/module/github.com/gin-gonic/gin", module)]);
    let client = RegistryClient::new(&http, &b);
    let page = go(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("go without go.mod");
    assert_eq!(page.artifacts[0].dependencies, None);
}

/// pkg.go.dev rejects a package path on the module endpoint (HTTP 400,
/// not 404): the lookup still reads the package endpoint and returns the
/// package row with its containing module; a path neither endpoint knows
/// keeps the module rejection.
#[tokio::test]
async fn go_package_paths_rejected_as_modules_read_the_package_endpoint() {
    let rejection = json_fixture(
        r#"{
            "code": 400,
            "message": "golang.org/x/net/http2 is a package, not a module"
        }"#,
    );
    let http = RouteHttp::new()
        .status("/v1/module/golang.org/x/net/http2", 400, rejection.clone())
        .json(
            "/v1/package/golang.org/x/net/http2",
            json_fixture(
                r#"{
                    "path": "golang.org/x/net/http2",
                    "modulePath": "golang.org/x/net",
                    "version": "v0.61.0",
                    "synopsis": "Package http2 implements the HTTP/2 protocol."
                }"#,
            ),
        );
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let q = exact_query(ArtifactType::Go, "golang.org/x/net/http2");
    let page = go(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("package fallback");
    let item = &page.artifacts[0];
    assert_eq!(item.package_name, "golang.org/x/net/http2");
    assert_eq!(item.module_path.as_deref(), Some("golang.org/x/net"));
    assert_eq!(item.version.as_deref(), Some("v0.61.0"));
    let paths = |http: &RouteHttp| {
        http.seen()
            .iter()
            .map(|seen| seen.url.path().to_owned())
            .collect::<Vec<_>>()
    };
    let lookups = [
        "/v1/module/golang.org/x/net/http2",
        "/v1/package/golang.org/x/net/http2",
    ];
    assert_eq!(paths(&http)[..2], lookups, "{:?}", paths(&http));
    let unknown = RouteHttp::new().status("/v1/module/golang.org/x/net/http2", 400, rejection);
    let client = RegistryClient::new(&unknown, &b);
    let error = go(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect_err("rejected by both endpoints");
    assert_eq!(error.status, Some(400));
    assert!(
        error.message.contains("is a package, not a module"),
        "{error:?}"
    );
    assert_eq!(paths(&unknown), lookups);
}

/// A pinned Go module version is looked up as that version, so its
/// release ref names the pinned tag, not the latest one; an unpublished
/// version names the nearest published ones, and ranges are refused.
#[tokio::test]
async fn go_pinned_versions_resolve_the_pinned_release() {
    let module = "/v1/module/github.com/open-telemetry/opentelemetry-go";
    let http = RouteHttp::routes(vec![
        (
            "/v1/module/github.com/open-telemetry/opentelemetry-go?version=v0.71.0",
            json_fixture(
                r#"{
                    "path": "github.com/open-telemetry/opentelemetry-go",
                    "version": "v0.71.0",
                    "repoUrl": "https://github.com/open-telemetry/opentelemetry-go"
                }"#,
            ),
        ),
        (
            module,
            json_fixture(
                r#"{
                    "path": "github.com/open-telemetry/opentelemetry-go",
                    "version": "v0.72.0",
                    "repoUrl": "https://github.com/open-telemetry/opentelemetry-go"
                }"#,
            ),
        ),
        (
            "/v1/versions/github.com/open-telemetry/opentelemetry-go",
            json_fixture(
                r#"{
                    "items": [
                        {"version": "v0.72.0"},
                        {"version": "v0.71.0"},
                        {"version": "v0.70.0"}
                    ]
                }"#,
            ),
        ),
    ]);
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let lookup = |version: Option<&str>| {
        let mut fields = json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "go",
                "packageName": "github.com/open-telemetry/opentelemetry-go"
            }"#,
        );
        if let Some(version) = version {
            fields["version"] = json!(version);
        }
        artifact_query(fields, None)
    };
    let state = ArtifactProviderState::default();
    for pinned in ["v0.71.0", "0.71.0"] {
        let page = go(&lookup(Some(pinned)), &state, &client)
            .await
            .expect("pinned");
        let item = &page.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("v0.71.0"));
        assert_eq!(item.source_ref.as_deref(), Some("v0.71.0"));
    }
    let latest = go(&lookup(Some("latest")), &state, &client)
        .await
        .expect("latest");
    assert_eq!(latest.artifacts[0].version.as_deref(), Some("v0.72.0"));
    let missing = go(&lookup(Some("v0.69.9")), &state, &client)
        .await
        .expect_err("unpublished");
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert!(
        missing.nearest.iter().any(|version| version == "0.70.0"),
        "{missing:?}"
    );
    assert!(
        octocode_runtime_base::contracts::validate_query(
            "artifactSearch",
            json_fixture(
                r#"{
                    "operation": "resolve",
                    "ecosystem": "go",
                    "packageName": "github.com/a/b",
                    "version": "^0.71"
                }"#
            )
        )
        .is_err()
    );
}

/// A vanity module path names neither its repository directory nor its
/// tag: the release commit and directory come from the module proxy.
#[tokio::test]
async fn go_vanity_modules_take_the_release_commit_from_the_proxy() {
    let http = RouteHttp::routes(vec![
        (
            "/v1/module/go.opentelemetry.io/otel/sdk?version=v1.30.0",
            json_fixture(
                r#"{
                    "path": "go.opentelemetry.io/otel/sdk",
                    "version": "v1.30.0",
                    "repoUrl": "https://github.com/open-telemetry/opentelemetry-go"
                }"#,
            ),
        ),
        (
            "/go.opentelemetry.io/otel/sdk/@v/v1.30.0.info",
            json_fixture(
                r#"{
                    "Version": "v1.30.0",
                    "Origin": {
                        "VCS": "git",
                        "URL": "https://github.com/open-telemetry/opentelemetry-go",
                        "Subdir": "sdk",
                        "Hash": "ed4fc757583a88b4da51b1fe1c3f0703ac27a487",
                        "Ref": "refs/tags/sdk/v1.30.0"
                    }
                }"#,
            ),
        ),
    ]);
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let query = artifact_query(
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "go",
                "packageName": "go.opentelemetry.io/otel/sdk",
                "version": "v1.30.0"
            }"#,
        ),
        None,
    );
    let page = go(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("vanity");
    let item = &page.artifacts[0];
    assert_eq!(
        item.source_ref.as_deref(),
        Some("ed4fc757583a88b4da51b1fe1c3f0703ac27a487")
    );
    assert_eq!(item.repository_directory.as_deref(), Some("sdk"));
    assert_eq!(
        go_proxy_escape("github.com/BurntSushi/toml"),
        "github.com/!burnt!sushi/toml"
    );
}

/// A vanity module with no recorded commit (gopkg.in/yaml.v2 at v2.2.8) is
/// pinned to its upstream release tag, which Go spells with its `v`.
#[tokio::test]
async fn a_vanity_module_without_a_commit_is_pinned_to_its_release_tag() {
    let http = RouteHttp::new().json(
        "/v1/module/gopkg.in/yaml.v2?version=v2.2.8",
        json_fixture(
            r#"{
                "path": "gopkg.in/yaml.v2",
                "version": "v2.2.8",
                "repoUrl": "https://github.com/go-yaml/yaml",
                "commitTime": "2019-12-03T00:00:00Z"
            }"#,
        ),
    );
    let b = test_budget();
    let tags = super::super::test_http::KnownTags::new(vec!["go-yaml/yaml@v2.2.8"]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Go,
            "packageName": "gopkg.in/yaml.v2", "version": "v2.2.8"}),
        None,
    );
    let page = go(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("yaml.v2");
    let item = &page.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some("commit-of-v2.2.8"));
    assert_eq!(item.release_tag.as_deref(), Some("v2.2.8"));
    assert!(item.source_tag);
    assert_eq!(tags.seen()[0], "go-yaml/yaml@v2.2.8", "{:?}", tags.seen());
}

#[test]
fn a_go_mod_module_comment_marks_the_module_deprecated() {
    let deprecated = "// Deprecated: Use the \"google.golang.org/protobuf\" module instead.\nmodule github.com/golang/protobuf\n\ngo 1.17\n";
    assert_eq!(
        go_deprecation(deprecated).as_deref(),
        Some("Use the \"google.golang.org/protobuf\" module instead.")
    );
    let trailing = "module example.com/old // Deprecated: use example.com/new\n";
    assert_eq!(go_deprecation(trailing).as_deref(), Some("use example.com/new"));
    let unrelated = "// Package docs.\nmodule example.com/live\n// Deprecated: not the module comment\n";
    assert_eq!(go_deprecation(unrelated), None);
}

/// A package inside a module (pkg.go.dev names no repository for it) takes
/// its repository from the module proxy, and the tag the proxy read is
/// checked upstream: found, it pins the row (`source_tag`); naming another
/// commit than the proxy's, the row says so.
#[tokio::test]
async fn a_package_path_takes_its_repository_and_checked_tag_from_the_proxy() {
    let rejection = json!({"code": 400, "message": "is a package, not a module"});
    let built = "bb548d0473d4e1c9b7bbfd6602c7bf12f7a84dd2";
    let http = RouteHttp::new()
        .status("/v1/module/github.com/stretchr/testify/assert", 400, rejection)
        .json(
            "/v1/package/github.com/stretchr/testify/assert",
            json!({"path": "github.com/stretchr/testify/assert",
                "modulePath": "github.com/stretchr/testify", "version": "v1.9.0"}),
        )
        .json(
            "/github.com/stretchr/testify/@v/v1.9.0.info",
            json!({"Version": "v1.9.0", "Origin": {"VCS": "git",
                "URL": "https://github.com/stretchr/testify",
                "Ref": "refs/tags/v1.9.0", "Hash": built}}),
        );
    let b = test_budget();
    let tags = super::super::test_http::KnownTags::new(vec!["stretchr/testify@v1.9.0"]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Go,
            "packageName": "github.com/stretchr/testify/assert", "version": "v1.9.0"}),
        None,
    );
    let page = go(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("package");
    let item = &page.artifacts[0];
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/stretchr/testify")
    );
    assert!(item.source_tag, "{item:?}");
    assert_eq!(item.release_tag.as_deref(), Some("v1.9.0"));
    assert_eq!(item.source_ref.as_deref(), Some("commit-of-v1.9.0"));
    assert!(
        item.disclosures
            .iter()
            .any(|said| said.contains(built) && said.contains("tag v1.9.0")),
        "{:?}",
        item.disclosures
    );
    // Without a tag check, the proxy's commit stays the registry's claim.
    let unchecked = RegistryClient::new(&http, &b);
    let page = go(&query, &ArtifactProviderState::default(), &unchecked)
        .await
        .expect("package");
    assert_eq!(page.artifacts[0].source_ref.as_deref(), Some(built));
    assert!(!page.artifacts[0].source_tag);
}

fn module_row(path: &str, version: &str, repository: &str) -> (String, serde_json::Value) {
    (
        format!("/v1/module/{path}?version={version}"),
        json!({"path": path, "version": version, "repoUrl": repository}),
    )
}

/// Tags checked on GitHub for `package` at `version`, where the proxy names no
/// commit and the repository has none of them.
async fn tag_checks(package: &str, version: &str, repository: &str) -> Vec<String> {
    let (target, body) = module_row(package, version, repository);
    let http = RouteHttp::new().json(&target, body);
    let b = test_budget();
    let tags = super::super::test_http::KnownTags::new(vec![]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Go,
            "packageName": package, "version": version}),
        None,
    );
    go(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("lookup");
    tags.seen()
        .into_iter()
        .filter(|seen| !seen.ends_with("@HEAD"))
        .collect()
}

/// A vanity module's tag is the go command's own spelling under its place in the
/// repository, never a name built from the vanity path or its major suffix.
#[tokio::test]
async fn a_vanity_module_probes_only_the_tags_its_repository_can_carry() {
    // The repository's root module: one tag.
    assert_eq!(
        tag_checks(
            "golang.org/x/net",
            "v0.17.0",
            "https://github.com/golang/net"
        )
        .await,
        ["golang/net@v0.17.0"]
    );
    assert_eq!(
        tag_checks(
            "go.uber.org/zap/v2",
            "v2.1.0",
            "https://github.com/uber-go/zap"
        )
        .await,
        ["uber-go/zap@v2.1.0"]
    );
    // A path that names the repository places the module below it.
    assert_eq!(
        tag_checks(
            "go.etcd.io/etcd/client/v3",
            "v3.5.9",
            "https://github.com/etcd-io/etcd"
        )
        .await,
        ["etcd-io/etcd@client/v3.5.9"]
    );
    // A path that never names the repository places it nowhere: the root tag.
    assert_eq!(
        tag_checks(
            "go.opentelemetry.io/otel/sdk",
            "v1.2.0",
            "https://github.com/open-telemetry/opentelemetry-go"
        )
        .await,
        ["open-telemetry/opentelemetry-go@v1.2.0"]
    );
}

#[test]
fn a_vanity_path_is_placed_by_the_segment_that_names_the_repository() {
    let place = |module: &str, repository: &str| {
        let mut item = ArtifactItem::new(ArtifactType::Go, module.into(), String::new());
        item.module_path = Some(module.into());
        item.repository = Some(repository.into());
        source::go_vanity_directory(&item)
    };
    assert_eq!(
        place("google.golang.org/grpc", "https://github.com/grpc/grpc-go"),
        Some(String::new())
    );
    assert_eq!(
        place(
            "google.golang.org/grpc/examples",
            "https://github.com/grpc/grpc-go"
        ),
        Some("examples".to_owned())
    );
    assert_eq!(
        place(
            "cloud.google.com/go/storage",
            "https://github.com/googleapis/google-cloud-go"
        ),
        None
    );
}

/// A module on GitHub checks the tag the go command spells for its version
/// and reads the source at the commit it names (gin v1.10.0: `tag`, not the
/// unchecked tag name as `registryRef`). A pseudo-version keeps its commit.
#[tokio::test]
async fn a_github_module_release_is_read_at_the_commit_its_tag_names() {
    let (target, body) = module_row(
        "github.com/gin-gonic/gin",
        "v1.10.0",
        "https://github.com/gin-gonic/gin",
    );
    let http = RouteHttp::new().json(&target, body);
    let b = test_budget();
    let tags = super::super::test_http::KnownTags::new(vec![
        "gin-gonic/gin@v1.10.0=75ccf94d605a05fe24817fc2f166f6f2959d5cea",
    ]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Go,
            "packageName": "github.com/gin-gonic/gin", "version": "v1.10.0"}),
        None,
    );
    let page = go(&query, &ArtifactProviderState::default(), &client)
        .await
        .expect("gin");
    let item = &page.artifacts[0];
    assert_eq!(
        item.source_ref.as_deref(),
        Some("75ccf94d605a05fe24817fc2f166f6f2959d5cea")
    );
    assert_eq!(item.release_tag.as_deref(), Some("v1.10.0"));
    assert!(item.source_tag);
    assert_eq!(tags.seen()[0], "gin-gonic/gin@v1.10.0", "{:?}", tags.seen());
}
