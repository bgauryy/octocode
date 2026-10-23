//! PyPI lookups: releases, requirements, licenses, yanked versions and pins.
use super::super::test_http::{KnownTags, RouteHttp};
use super::super::types::{exact_query, test_budget, versioned};
use super::*;

use serde_json::json;

fn pypi_release(version: &str) -> serde_json::Value {
    json!({
        "info": {"name": "requests", "version": version, "summary": "HTTP",
            "requires_python": ">=3.7", "yanked": false,
            "requires_dist": ["idna<4,>=2.5", "urllib3<3", "PySocks!=1.5.7; extra == \"socks\""],
            "project_urls": {"Source": "https://github.com/psf/requests"}},
        "urls": [{"upload_time_iso_8601": "2023-05-22T15:12:42.313790Z"}]
    })
}

fn pypi_project() -> serde_json::Value {
    let mut project = pypi_release("2.32.3");
    project["releases"] = json!({
        "2.30.0": [{"yanked": false}], "2.31.0": [{"yanked": false}],
        "2.32.0": [{"yanked": true}], "2.32.3": [{"yanked": false}], "3.0.0rc1": [{"yanked": false}]
    });
    project
}

#[tokio::test]
async fn pypi_version_lookups_pin_exact_specifier_and_coordinate_forms() {
    let http = RouteHttp::routes(vec![
        ("/pypi/requests/json", pypi_project()),
        ("/pypi/requests/2.31.0/json", pypi_release("2.31.0")),
        ("/pypi/requests/2.32.3/json", pypi_release("2.32.3")),
    ]);
    let tags = KnownTags::new(vec!["psf/requests@v2.31.0"]);
    let b = test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &b,
        cache: None,
        tags: Some(&tags),
    };
    let exact = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.31.0"),
        &client,
    )
    .await
    .expect("exact");
    let item = &exact.artifacts[0];
    assert_eq!(item.version.as_deref(), Some("2.31.0"));
    assert_eq!(item.published_at.as_deref(), Some("2023-05-22"));
    assert_eq!(item.requires_python.as_deref(), Some(">=3.7"));
    assert_eq!(
        item.dependencies.as_ref().map(Vec::len),
        Some(2),
        "extras are not dependencies: {:?}",
        item.dependencies
    );
    assert_eq!(
        item.optional_dependencies,
        Some(vec!["PySocks!=1.5.7; extra == \"socks\"".to_owned()]),
        "extras are optional dependencies"
    );
    assert_eq!(item.yanked, None);
    // The upstream release tag pins the source lead.
    assert_eq!(item.source_ref.as_deref(), Some("v2.31.0"));
    let coordinate = pypi(
        &exact_query(ArtifactType::Pypi, "requests==2.31.0"),
        &client,
    )
    .await
    .expect("== coordinate");
    assert_eq!(coordinate.artifacts[0].version.as_deref(), Some("2.31.0"));
    let range = pypi(
        &versioned(ArtifactType::Pypi, "requests", ">=2.31,<3"),
        &client,
    )
    .await
    .expect("specifier");
    assert_eq!(range.artifacts[0].version.as_deref(), Some("2.32.3"));
    // Neither `v2.32.3` nor `2.32.3` exists upstream: no release ref.
    assert_eq!(range.artifacts[0].source_ref, None);
    let missing = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.31.9"),
        &client,
    )
    .await
    .expect_err("missing version");
    assert_eq!(missing.code, ErrorCode::VersionNotFound);
    assert!(
        missing.nearest.iter().any(|version| version == "2.31.0"),
        "{missing:?}"
    );
    let registry: Vec<String> = http
        .targets()
        .into_iter()
        .filter(|path| path.starts_with("/pypi/"))
        .collect();
    assert_eq!(
        registry[..2],
        ["/pypi/requests/2.31.0/json", "/pypi/requests/2.31.0/json"],
        "exact versions are one registry request"
    );
    assert_eq!(
        tags.seen()[..2],
        ["psf/requests@v2.31.0", "psf/requests@2.31.0"],
        "the plain tags are checked together, and the `v` tag wins"
    );
    assert_eq!(tags.seen()[2], "psf/requests@v2.31.0");
}

/// PEP 592: an `==`/`===` pin selects a yanked release; a range that
/// only yanked releases match says so and offers the newest one.
#[tokio::test]
async fn pypi_pins_select_yanked_releases_and_ranges_name_them() {
    let mut yanked = pypi_release("2.32.0");
    yanked["info"]["yanked"] = json!(true);
    let http = RouteHttp::routes(vec![
        ("/pypi/requests/json", pypi_project()),
        ("/pypi/requests/2.32.0/json", yanked),
    ]);
    let b = test_budget();
    let client = RegistryClient::uncached(&http, &b);
    for pin in ["==2.32.0", "===2.32.0", "== 2.32.0"] {
        let page = pypi(&versioned(ArtifactType::Pypi, "requests", pin), &client)
            .await
            .unwrap_or_else(|error| panic!("{pin}: {error:?}"));
        assert_eq!(
            page.artifacts[0].version.as_deref(),
            Some("2.32.0"),
            "{pin}"
        );
        assert_eq!(page.artifacts[0].yanked, Some(true), "{pin}");
    }
    let only_yanked = pypi(
        &versioned(ArtifactType::Pypi, "requests", ">=2.32.0,<2.32.2"),
        &client,
    )
    .await
    .expect_err("only a yanked release matches");
    assert_eq!(only_yanked.code, ErrorCode::VersionNotFound);
    assert!(
        only_yanked.message.contains("yanked 2.32.0"),
        "{only_yanked:?}"
    );
    assert_eq!(only_yanked.nearest, ["2.32.0"]);
}

/// The row's license is a short identifier: the SPDX expression, a
/// one-line license field, else the license classifier; never the full
/// license text some projects put in `license`.
#[tokio::test]
async fn pypi_license_is_short_never_the_license_text() {
    let text = format!(
        "Copyright (c) 2005-2024, NumPy Developers.\nAll rights reserved.\n{}",
        "Redistribution and use in source and binary forms. ".repeat(40)
    );
    let cases = [
        (
            json!({"license_expression": "BSD-3-Clause", "license": text}),
            Some("BSD-3-Clause"),
        ),
        (json!({"license": "Apache 2.0"}), Some("Apache 2.0")),
        (
            json!({"license": text, "classifiers": [
                "Programming Language :: Python",
                "License :: OSI Approved :: BSD License"
            ]}),
            Some("BSD License"),
        ),
        (json!({"license": text}), None),
    ];
    for (fields, expected) in cases {
        let mut release = pypi_release("2.31.0");
        for (key, value) in fields.as_object().expect("fields") {
            release["info"][key] = value.clone();
        }
        let http = RouteHttp::routes(vec![("/pypi/requests/2.31.0/json", release)]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let page = pypi(
            &versioned(ArtifactType::Pypi, "requests", "2.31.0"),
            &client,
        )
        .await
        .expect("release");
        assert_eq!(page.artifacts[0].license.as_deref(), expected, "{fields}");
    }
}

/// A crate version's source ref is the commit cargo packaged it from,
/// with its directory in the repository; without VCS info it is the
/// upstream release tag, when one exists.
/// A monorepo package whose release tag carries a prefix other than its
/// full name (pydantic-core: `core-v2.50.1` in pydantic/pydantic) still
/// pins the release lead to that tag.
#[tokio::test]
async fn a_monorepo_pypi_release_is_pinned_to_its_prefixed_tag() {
    let mut release = pypi_release("2.50.1");
    release["info"]["name"] = json!("pydantic_core");
    release["info"]["project_urls"] = json!({
        "Source": "https://github.com/pydantic/pydantic/tree/main/pydantic-core"
    });
    let http = RouteHttp::routes(vec![("/pypi/pydantic-core/2.50.1/json", release)]);
    let tags = KnownTags::new(vec!["pydantic/pydantic@core-v2.50.1"]);
    let b = test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &b,
        cache: None,
        tags: Some(&tags),
    };
    let page = pypi(
        &versioned(ArtifactType::Pypi, "pydantic-core", "2.50.1"),
        &client,
    )
    .await
    .expect("exact");
    assert_eq!(
        page.artifacts[0].source_ref.as_deref(),
        Some("core-v2.50.1")
    );
    assert!(page.artifacts[0].source_tag);
}

/// Only requirements an extra gates are optional; an environment
/// marker alone keeps a requirement required.
#[test]
fn pypi_requirements_gated_by_an_extra_are_optional() {
    for optional in [
        "PySocks!=1.5.7; extra == \"socks\"",
        "x; extra=='y'",
        "x ; python_version < \"4\" and extra == \"y\"",
    ] {
        assert!(names_an_extra(optional), "{optional}");
    }
    for required in [
        "idna<4,>=2.5",
        "tzdata; sys_platform == \"win32\"",
        "x; my_extra == \"y\"",
    ] {
        assert!(!names_an_extra(required), "{required}");
    }
}

/// A found release tag is a release fact: the same release looked up
/// again checks no tag on GitHub.
#[tokio::test]
async fn found_release_tags_are_remembered() {
    let http = RouteHttp::routes(vec![
        ("/pypi/requests/json", pypi_project()),
        ("/pypi/requests/2.31.0/json", pypi_release("2.31.0")),
    ]);
    let tags = KnownTags::new(vec!["psf/requests@v2.31.0"]);
    let cache = super::super::ArtifactCache::new(None);
    let b = test_budget();
    let client = RegistryClient {
        http: &http,
        budget: &b,
        cache: Some(&cache),
        tags: Some(&tags),
    };
    let query = versioned(ArtifactType::Pypi, "requests", "2.31.0");
    for _ in 0..2 {
        let page = pypi(&query, &client).await.expect("exact");
        assert_eq!(page.artifacts[0].source_ref.as_deref(), Some("v2.31.0"));
        assert!(page.artifacts[0].source_tag);
    }
    assert_eq!(
        tags.seen(),
        ["psf/requests@v2.31.0", "psf/requests@2.31.0"],
        "one round of tag checks"
    );
}
