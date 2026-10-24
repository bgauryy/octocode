//! NuGet exact lookups and catalog rows: stable releases, inline items, nuspec repositories and version order.
use super::super::query::artifact_query;
use super::super::test_http::RouteHttp;
use super::super::test_http::test_budget;
use super::entries::repository_url;
use super::*;
use serde_json::json;

/// The exact `Newtonsoft.Json` lookup against `http`.
async fn newtonsoft_exact(http: &RouteHttp) -> ArtifactProviderPage {
    let b = test_budget();
    let client = RegistryClient::uncached(http, &b);
    let q = artifact_query(
        serde_json::json!({"operation":"resolve","ecosystem": ArtifactType::Nuget, "packageName": "Newtonsoft.Json".to_string()}),
        None,
    );
    nuget(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("nuget exact")
}

const REGISTRATION: &str = "/v3/registration5/newtonsoft.json/index.json";

fn service_index() -> serde_json::Value {
    json!({
        "resources": [
            {
                "@type": "RegistrationsBaseUrl/3.6.0",
                "@id": "https://api.nuget.org/v3/registration5/"
            }
        ]
    })
}

#[tokio::test]
async fn nuget_exact_prefers_the_latest_stable_release_and_reads_the_nuspec_repository() {
    let registration = json!({"items": [{
        "upper": "14.0.1-beta2",
        "items": [
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.4"}},
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "14.0.1-beta2"}},
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.5", "listed": false}}
        ]
    }]});
    let mut index = service_index();
    index["resources"].as_array_mut().unwrap().push(json!({
        "@type": "PackageBaseAddress/3.0.0",
        "@id": "https://api.nuget.org/v3-flatcontainer/"
    }));
    let http = RouteHttp::routes(vec![
        (REGISTRATION, registration),
        ("/v3/index.json", index),
    ])
    .bytes(
        "/v3-flatcontainer/newtonsoft.json/13.0.4/newtonsoft.json.nuspec",
        br#"<package><metadata><repository type="git" url="https://github.com/JamesNK/Newtonsoft.Json" commit="0A2E291C0D9C0C7675D445703E51750363A549EF" /></metadata></package>"#.to_vec(),
    );
    let page = newtonsoft_exact(&http).await;
    let item = &page.artifacts[0];
    // 13.0.5 is unlisted; 14.0.1-beta2 is a prerelease.
    assert_eq!(item.version.as_deref(), Some("13.0.4"));
    // The nuspec's build commit pins the upstream-source lead.
    assert_eq!(
        item.source_ref.as_deref(),
        Some("0a2e291c0d9c0c7675d445703e51750363a549ef")
    );
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/JamesNK/Newtonsoft.Json")
    );
}

#[tokio::test]
async fn nuget_parses_exact_inline_items() {
    // GET https://api.nuget.org/v3/registration5/newtonsoft.json/index.json
    // Items are inline so no further HTTP call is needed.
    let registration = json!({
        "items": [
            {
                "upper": "13.0.3",
                "items": [
                    {
                        "catalogEntry": {
                            "id": "Newtonsoft.Json",
                            "version": "13.0.3",
                            "description": "Json.NET is a popular high-performance JSON framework",
                            "projectUrl": "https://www.newtonsoft.com/json",
                            "licenseExpression": "MIT"
                        }
                    }
                ]
            }
        ]
    });
    let http = RouteHttp::routes(vec![
        (REGISTRATION, registration),
        ("/v3/index.json", service_index()),
    ]);
    let page = newtonsoft_exact(&http).await;
    assert_eq!(page.artifacts.len(), 1);
    let item = &page.artifacts[0];
    assert_eq!(item.package_name, "Newtonsoft.Json");
    assert_eq!(item.version.as_deref(), Some("13.0.3"));
    assert_eq!(
        item.registry_url,
        "https://www.nuget.org/packages/Newtonsoft.Json"
    );
    // Live catalogEntry payloads omit `repository` (verified against
    // registration5-gz-semver2 for Newtonsoft.Json). A marketing
    // projectUrl is a homepage, never a verified source repository.
    assert_eq!(item.repository, None);
    assert_eq!(
        item.homepage.as_deref(),
        Some("https://www.newtonsoft.com/json")
    );
}

/// An exact version reads the registration page whose bounds hold it
/// (inline or linked), even when a newer release exists; a missing one
/// names the nearest published versions; a range is refused.
#[tokio::test]
async fn nuget_exact_version_reads_the_page_that_holds_it() {
    let registration = json!({"items": [
        {"lower": "13.0.1", "upper": "13.0.4", "items": [
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.1"}},
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.3", "licenseExpression": "MIT"}},
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.4"}}
        ]},
        {"lower": "9.0.1", "upper": "12.0.3",
         "@id": "https://api.nuget.org/v3/registration5/newtonsoft.json/page/9.0.1/12.0.3.json"}
    ]});
    let old_page = json!({"items": [
        {"catalogEntry": {"id": "Newtonsoft.Json", "version": "9.0.1"}},
        {"catalogEntry": {"id": "Newtonsoft.Json", "version": "12.0.3"}}
    ]});
    let http = RouteHttp::routes(vec![
        (
            "/v3/registration5/newtonsoft.json/page/9.0.1/12.0.3.json",
            old_page,
        ),
        (REGISTRATION, registration),
        ("/v3/index.json", service_index()),
    ]);
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let lookup = |version: &str| {
        artifact_query(
            json!({"operation":"resolve","ecosystem": ArtifactType::Nuget, "packageName": "Newtonsoft.Json", "version": version}),
            None,
        )
    };
    let state = ArtifactProviderState::default();
    for (requested, expected) in [
        ("13.0.3", "13.0.3"),
        ("12.0.3", "12.0.3"),
        ("v13.0.3", "13.0.3"),
        ("13.0.3.0", "13.0.3"),
        ("latest", "13.0.4"),
    ] {
        let page = nuget(&lookup(requested), &state, &client)
            .await
            .unwrap_or_else(|error| panic!("{requested}: {error:?}"));
        assert_eq!(
            page.artifacts[0].version.as_deref(),
            Some(expected),
            "{requested}"
        );
    }
    let missing = nuget(&lookup("13.0.2"), &state, &client)
        .await
        .expect_err("missing version");
    assert_eq!(missing.code, ErrorCode::VersionNotFound, "{missing:?}");
    assert!(
        missing.nearest.iter().any(|version| version == "13.0.3"),
        "{missing:?}"
    );
    assert!(crate::contracts::validate_query("artifactSearch", json!({
        "operation":"resolve", "ecosystem":"nuget", "packageName":"Newtonsoft.Json", "version":"[13.0,14.0)"
    })).is_err());
}

/// An exact row states its publish day and its dependency groups'
/// entries once each; one only some frameworks declare names them. An
/// unlisted release's placeholder day (1900) is no day, and a release
/// without dependency groups declares none.
#[tokio::test]
async fn nuget_exact_rows_state_the_publish_day_and_dependencies() {
    let dependency = |id: &str, range: &str| json!({"id": id, "range": range});
    let registration = json!({"items": [{"upper": "4.4.0", "items": [
        {"catalogEntry": {"id": "Newtonsoft.Json", "version": "4.4.0",
            "published": "2026-07-10T03:35:24.047+00:00",
            "dependencyGroups": [
                {"targetFramework": "net10.0"},
                {"targetFramework": ".NETFramework4.6.2", "dependencies": [
                    dependency("System.Diagnostics.DiagnosticSource", "[8.0.1, )"),
                    dependency("System.ValueTuple", "[4.5.0, )")]},
                {"targetFramework": "net8.0", "dependencies": [
                    dependency("System.Diagnostics.DiagnosticSource", "[8.0.1, )")]},
                {"dependencies": [dependency("Everywhere", "[1.0.0, )")]}
            ]}}
    ]}]});
    let http = RouteHttp::routes(vec![
        (REGISTRATION, registration),
        ("/v3/index.json", service_index()),
    ]);
    let item = &newtonsoft_exact(&http).await.artifacts[0];
    assert_eq!(item.published_at.as_deref(), Some("2026-07-10"));
    assert_eq!(
        item.dependencies,
        Some(vec![
            "System.Diagnostics.DiagnosticSource [8.0.1, ) (.NETFramework4.6.2, net8.0)".to_owned(),
            "System.ValueTuple [4.5.0, ) (.NETFramework4.6.2)".to_owned(),
            "Everywhere [1.0.0, )".to_owned(),
        ])
    );
    let unlisted = json!({"items": [{"upper": "1.0.0", "items": [
        {"catalogEntry": {"id": "Newtonsoft.Json", "version": "1.0.0",
            "published": "1900-01-01T00:00:00+00:00"}}
    ]}]});
    let http = RouteHttp::routes(vec![
        (REGISTRATION, unlisted),
        ("/v3/index.json", service_index()),
    ]);
    let item = &newtonsoft_exact(&http).await.artifacts[0];
    assert_eq!(item.published_at, None);
    assert_eq!(item.dependencies, Some(Vec::new()));
}

#[test]
fn project_url_is_a_repository_only_on_a_source_host() {
    let row = |project: &str| {
        json!({"id": "P", "projectUrl": project})
            .as_object()
            .cloned()
            .unwrap()
    };
    let github = item(&row("https://github.com/o/r")).unwrap();
    assert_eq!(github.repository.as_deref(), Some("https://github.com/o/r"));
    for homepage in [
        "https://www.newtonsoft.com/json",
        "https://github.com/o",
        "https://github.com.evil.example/o/r",
    ] {
        let artifact = item(&row(homepage)).unwrap();
        assert_eq!(artifact.repository, None, "{homepage}");
        assert_eq!(artifact.homepage.as_deref(), Some(homepage));
    }
}

#[tokio::test]
async fn nuget_prefers_repository_object_over_project_url() {
    let registration = json!({
        "items": [
            {
                "upper": "4.4.0",
                "items": [
                    {
                        "catalogEntry": {
                            "id": "Serilog",
                            "version": "4.4.0",
                            "projectUrl": "https://serilog.net/",
                            "repository": {
                                "type": "git",
                                "url": "https://github.com/serilog/serilog"
                            }
                        }
                    }
                ]
            }
        ]
    });
    let http = RouteHttp::routes(vec![
        ("/v3/registration5/serilog/index.json", registration),
        ("/v3/index.json", service_index()),
    ]);
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    let q = artifact_query(
        serde_json::json!({"operation":"resolve","ecosystem": ArtifactType::Nuget, "packageName": "Serilog".to_string()}),
        None,
    );
    let page = nuget(&q, &ArtifactProviderState::default(), &client)
        .await
        .expect("nuget exact");
    let item = &page.artifacts[0];
    assert_eq!(
        item.repository.as_deref(),
        Some("https://github.com/serilog/serilog")
    );
    assert_eq!(item.homepage.as_deref(), Some("https://serilog.net/"));
}

#[test]
fn repository_url_accepts_object_and_string_shapes() {
    assert_eq!(
        repository_url(Some(&json!({"url": "https://github.com/a/b"}))).as_deref(),
        Some("https://github.com/a/b")
    );
    assert_eq!(
        repository_url(Some(&json!("https://github.com/a/b"))).as_deref(),
        Some("https://github.com/a/b")
    );
    // Empty-string repository (common in NuGet catalog data) yields None.
    assert_eq!(repository_url(Some(&json!(""))), None);
    assert_eq!(repository_url(None), None);
}

#[test]
fn compare_versions_orders_semver_correctly() {
    use std::cmp::Ordering;
    // stable > prerelease
    assert_eq!(
        compare_versions("13.0.3", "13.0.3-beta1").expect("NuGet test data should be valid"),
        Ordering::Greater
    );
    // higher patch wins
    assert_eq!(
        compare_versions("2.0.1", "2.0.0").expect("NuGet test data should be valid"),
        Ordering::Greater
    );
    // equal
    assert_eq!(
        compare_versions("1.0.0", "1.0.0").expect("NuGet test data should be valid"),
        Ordering::Equal
    );
    // 4-part version
    assert_eq!(
        compare_versions("1.2.3.4", "1.2.3.3").expect("NuGet test data should be valid"),
        Ordering::Greater
    );
}
