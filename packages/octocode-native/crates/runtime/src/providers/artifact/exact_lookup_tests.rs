//! Exact package lookups through the registry client.
use super::http::RegistryClient;
use super::query::exact_query;
use super::test_http::RouteHttp;
use super::test_http::test_budget;
use super::*;
use serde_json::json;

/// An exact lookup on each registry parses one row with its name,
/// version and registry page.
#[tokio::test]
async fn exact_lookups_parse_one_row_per_registry() {
    let cases = [
        (
            ArtifactType::Pypi,
            "requests",
            json!({
                "info": {
                    "name": "requests",
                    "version": "2.31.0",
                    "summary": "Python HTTP for Humans.",
                    "license": "Apache-2.0",
                    "home_page": "https://requests.readthedocs.io",
                    "project_urls": {"Source": "https://github.com/psf/requests"}
                }
            }),
            "2.31.0",
            "https://pypi.org/project/requests/",
        ),
        (
            ArtifactType::Crates,
            "serde",
            json!({
                "crate": {
                    "id": "serde",
                    "name": "serde",
                    "max_stable_version": "1.0.200",
                    "description": "A generic serialization/deserialization framework",
                    "license": "MIT OR Apache-2.0",
                    "repository": "https://github.com/serde-rs/serde"
                }
            }),
            "1.0.200",
            "https://crates.io/crates/serde",
        ),
        (
            ArtifactType::Packagist,
            "laravel/framework",
            // p2/{name}.json response with entries for the tagged URL
            json!({
                "packages": {
                    "laravel/framework": [
                        {
                            "name": "laravel/framework",
                            "version": "10.0.0",
                            "description": "The Laravel Framework.",
                            "license": ["MIT"]
                        }
                    ]
                }
            }),
            "10.0.0",
            "https://packagist.org/packages/laravel/framework",
        ),
        (
            ArtifactType::Rubygems,
            "rails",
            json!({
                "name": "rails",
                "version": "7.0.6",
                "info": "Full-stack web application framework.",
                "homepage_uri": "https://rubyonrails.org",
                "source_code_uri": "https://github.com/rails/rails"
            }),
            "7.0.6",
            "https://rubygems.org/gems/rails",
        ),
    ];
    for (ecosystem, name, body, version, registry_url) in cases {
        let http = RouteHttp::always(body);
        let b = test_budget();
        let client = RegistryClient::new(&http, &b);
        let q = exact_query(ecosystem, name);
        let state = ArtifactProviderState::default();
        let page = match ecosystem {
            ArtifactType::Pypi => pypi::pypi(&q, &client).await,
            ArtifactType::Crates => crates::crates(&q, &state, &client).await,
            ArtifactType::Packagist => packagist::packagist(&q, &state, &client).await,
            ArtifactType::Rubygems => rubygems::rubygems(&q, &state, &client).await,
            other => unreachable!("no exact-lookup case for {other:?}"),
        }
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(page.artifacts.len(), 1, "{name}");
        let item = &page.artifacts[0];
        assert_eq!(item.package_name, name);
        assert_eq!(item.version.as_deref(), Some(version), "{name}");
        assert_eq!(item.registry_url, registry_url, "{name}");
    }
}

/// The p2 document of `symfony/http-kernel`, minified as Packagist serves it.
fn minified_p2() -> serde_json::Value {
    json!({"minified": "composer/2.0", "packages": {"symfony/http-kernel": [
        {"name": "symfony/http-kernel", "version": "v8.1.11", "description": "kept",
         "license": ["MIT"], "time": "2026-10-01T00:00:00+00:00"},
        {"version": "v4.4.0", "time": "2019-11-21T00:00:00+00:00"}
    ]}})
}

async fn packagist_at(
    http: &RouteHttp,
    version: &str,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let b = test_budget();
    let client = RegistryClient::new(http, &b);
    let q = super::query::versioned(ArtifactType::Packagist, "symfony/http-kernel", version);
    packagist::packagist(&q, &ArtifactProviderState::default(), &client).await
}

/// A Packagist row at a named version: spelled with or without its `v`, whole
/// from a minified document; a version the package lacks names the nearest.
#[tokio::test]
async fn packagist_resolves_an_exact_version() {
    let p2 = "/p2/symfony/http-kernel.json";
    let http = RouteHttp::new().json(p2, minified_p2());
    for spelled in ["v4.4.0", "4.4.0"] {
        let page = packagist_at(&http, spelled).await.expect("release");
        let item = &page.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("v4.4.0"), "{spelled}");
        assert_eq!(item.published_at.as_deref(), Some("2019-11-21"));
        assert_eq!(item.description.as_deref(), Some("kept"), "inherited");
    }
    let error = packagist_at(&http, "9.9.9").await.expect_err("absent");
    assert_eq!(error.code, ErrorCode::VersionNotFound);
    assert!(!error.nearest.is_empty(), "{error:?}");
    // A dev version lives in the `~dev` document.
    let http = RouteHttp::new().json(p2, minified_p2()).json(
        "/p2/symfony/http-kernel~dev.json",
        json!({"packages": {"symfony/http-kernel": [
            {"name": "symfony/http-kernel", "version": "dev-main"}
        ]}}),
    );
    let page = packagist_at(&http, "dev-main").await.expect("dev");
    assert_eq!(page.artifacts[0].version.as_deref(), Some("dev-main"));
    // An unknown package is empty, never a version miss.
    let missing = RouteHttp::new();
    let page = packagist_at(&missing, "1.0.0").await.expect("empty");
    assert!(page.artifacts.is_empty());
}

async fn rubygems_at(
    http: &RouteHttp,
    version: &str,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let b = test_budget();
    let client = RegistryClient::new(http, &b);
    let q = super::query::versioned(ArtifactType::Rubygems, "rack", version);
    rubygems::rubygems(&q, &ArtifactProviderState::default(), &client).await
}

/// A RubyGems row at a named version, from its own document. A miss there says
/// neither gem nor version is known: the versions list tells them apart.
#[tokio::test]
async fn rubygems_resolves_an_exact_version() {
    let exact = "/api/v2/rubygems/rack/versions/2.2.3.json";
    let http = RouteHttp::new().json(
        exact,
        json!({
            "name": "rack", "version": "2.2.3", "info": "A modular Ruby webserver interface.",
            "licenses": ["MIT"], "version_created_at": "2020-06-15T22:25:14.574Z",
            "source_code_uri": "https://github.com/rack/rack",
            "dependencies": {"runtime": [{"name": "dep", "requirements": ">= 0"}]}
        }),
    );
    let page = rubygems_at(&http, "2.2.3").await.expect("release");
    let item = &page.artifacts[0];
    assert_eq!(item.version.as_deref(), Some("2.2.3"));
    assert_eq!(item.published_at.as_deref(), Some("2020-06-15"));
    assert_eq!(item.dependencies, Some(vec!["dep >= 0".to_owned()]));
    assert_eq!(http.targets(), [exact]);
    let listed = RouteHttp::new().json(
        "/api/v1/versions/rack.json",
        json!([{"number": "3.2.8"}, {"number": "2.2.3"}]),
    );
    let error = rubygems_at(&listed, "2.2.4").await.expect_err("absent");
    assert_eq!(error.code, ErrorCode::VersionNotFound);
    assert!(
        error.nearest.iter().any(|near| near == "2.2.3"),
        "{error:?}"
    );
    let missing = RouteHttp::new();
    let page = rubygems_at(&missing, "1.0.0").await.expect("empty");
    assert!(page.artifacts.is_empty());
}
