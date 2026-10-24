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
        let client = RegistryClient::uncached(&http, &b);
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
