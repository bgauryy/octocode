//! Go exact lookups: source refs, module directories, package paths, pins and vanity modules.
use super::super::test_http::RouteHttp;
use super::super::types::{artifact_query, exact_query, test_budget};
use super::*;

use serde_json::json;

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

/// A module inside a repository keeps its directory, so the release
/// lead lists the module and not the repository root.
#[test]
fn go_module_directory_is_the_path_under_the_repository_root() {
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
    let module = json!({
        "path": "github.com/gin-gonic/gin",
        "version": "v1.9.1",
        "synopsis": "HTTP web framework for Go",
        "commitTime": "2023-06-12T08:50:28Z"
    });
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
    let client = RegistryClient::uncached(&http, &b);
    let page = go(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("go");
    assert_eq!(page.artifacts.len(), 1);
    let item = &page.artifacts[0];
    assert_eq!(item.name, "github.com/gin-gonic/gin");
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
    let client = RegistryClient::uncached(&http, &b);
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
    let rejection = json!({"code": 400,
        "message": "golang.org/x/net/http2 is a package, not a module"});
    let http = RouteHttp::new()
        .status("/v1/module/golang.org/x/net/http2", 400, rejection.clone())
        .json(
            "/v1/package/golang.org/x/net/http2",
            json!({"path": "golang.org/x/net/http2", "modulePath": "golang.org/x/net",
                "version": "v0.61.0", "synopsis": "Package http2 implements the HTTP/2 protocol."}),
        );
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let q = exact_query(ArtifactType::Go, "golang.org/x/net/http2");
    let page = go(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("package fallback");
    let item = &page.artifacts[0];
    assert_eq!(item.name, "golang.org/x/net/http2");
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
    let client = RegistryClient::uncached(&unknown, &b);
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
            json!({"path":"github.com/open-telemetry/opentelemetry-go","version":"v0.71.0",
                "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
        ),
        (
            module,
            json!({"path":"github.com/open-telemetry/opentelemetry-go","version":"v0.72.0",
                "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
        ),
        (
            "/v1/versions/github.com/open-telemetry/opentelemetry-go",
            json!({"items":[{"version":"v0.72.0"},{"version":"v0.71.0"},{"version":"v0.70.0"}]}),
        ),
    ]);
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let lookup = |version: Option<&str>| {
        let mut fields =
            json!({"ecosystem":"go","packageName":"github.com/open-telemetry/opentelemetry-go"});
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
    assert_eq!(missing.code, ErrorCode::VersionNotFound);
    assert!(
        missing.nearest.iter().any(|version| version == "0.70.0"),
        "{missing:?}"
    );
    let range = go(&lookup(Some("^0.71")), &state, &client)
        .await
        .expect_err("range");
    assert_eq!(range.code, ErrorCode::CapabilityUnavailable);
}

/// A vanity module path names neither its repository directory nor its
/// tag: the release commit and directory come from the module proxy.
#[tokio::test]
async fn go_vanity_modules_take_the_release_commit_from_the_proxy() {
    let http = RouteHttp::routes(vec![
        (
            "/v1/module/go.opentelemetry.io/otel/sdk?version=v1.30.0",
            json!({"path":"go.opentelemetry.io/otel/sdk","version":"v1.30.0",
                "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
        ),
        (
            "/go.opentelemetry.io/otel/sdk/@v/v1.30.0.info",
            json!({"Version":"v1.30.0","Origin":{"VCS":"git",
                "URL":"https://github.com/open-telemetry/opentelemetry-go","Subdir":"sdk",
                "Hash":"ed4fc757583a88b4da51b1fe1c3f0703ac27a487","Ref":"refs/tags/sdk/v1.30.0"}}),
        ),
    ]);
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let query = artifact_query(
        json!({"ecosystem":"go","packageName":"go.opentelemetry.io/otel/sdk","version":"v1.30.0"}),
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
