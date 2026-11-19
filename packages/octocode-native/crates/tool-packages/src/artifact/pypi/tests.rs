//! PyPI lookups: releases, requirements, licenses, yanked versions and pins.
use super::super::query::versioned;
use super::super::test_http::test_budget;
use super::super::test_http::{KnownTags, RouteHttp};
use super::*;

mod advisories;

use octocode_test_support::json_fixture;
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
    project["releases"] = json_fixture(
        r#"{
            "2.30.0": [{"yanked": false}],
            "2.31.0": [{"yanked": false}],
            "2.32.0": [{"yanked": true}],
            "2.32.3": [{"yanked": false}],
            "3.0.0rc1": [{"yanked": false}]
        }"#,
    );
    project
}

#[tokio::test]
async fn pypi_version_lookups_pin_exact_and_specifier_forms() {
    let http = RouteHttp::routes(vec![
        ("/pypi/requests/json", pypi_project()),
        ("/pypi/requests/2.31.0/json", pypi_release("2.31.0")),
        ("/pypi/requests/2.32.3/json", pypi_release("2.32.3")),
    ]);
    let tags = KnownTags::new(vec!["psf/requests@v2.31.0"]);
    let b = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let exact = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.31.0"),
        &client,
    )
    .await
    .expect("exact");
    assert_eq!(
        http.targets()
            .iter()
            .filter(|path| path.starts_with("/pypi/"))
            .collect::<Vec<_>>(),
        vec!["/pypi/requests/2.31.0/json"]
    );
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
    assert_eq!(item.source_ref.as_deref(), Some("commit-of-v2.31.0"));
    assert_eq!(item.release_tag.as_deref(), Some("v2.31.0"));
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
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert!(
        missing.nearest.iter().any(|version| version == "2.31.0"),
        "{missing:?}"
    );
    let registry: Vec<String> = http
        .targets()
        .into_iter()
        .filter(|path| path.starts_with("/pypi/"))
        .collect();
    assert!(registry.iter().any(|path| path == "/pypi/requests/json"));
    assert!(
        registry
            .iter()
            .any(|path| path == "/pypi/requests/2.32.3/json")
    );
    assert!(
        registry
            .iter()
            .any(|path| path == "/pypi/requests/2.31.9/json")
    );
    assert_eq!(
        tags.seen()[..2],
        ["psf/requests@v2.31.0", "psf/requests@2.31.0"],
        "the plain tags are checked together, and the `v` tag wins"
    );
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
    let client = RegistryClient::new(&http, &b);
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
    assert_eq!(only_yanked.code, ErrorCode::NotFound);
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
        (
            json_fixture(r#"{"license": "Apache 2.0"}"#),
            Some("Apache 2.0"),
        ),
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
        let client = RegistryClient::new(&http, &b);
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
    release["info"]["project_urls"] = json_fixture(
        r#"{
            "Source": "https://github.com/pydantic/pydantic/tree/main/pydantic-core"
        }"#,
    );
    let http = RouteHttp::routes(vec![("/pypi/pydantic-core/2.50.1/json", release)]);
    let tags = KnownTags::new(vec!["pydantic/pydantic@core-v2.50.1"]);
    let b = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let page = pypi(
        &versioned(ArtifactType::Pypi, "pydantic-core", "2.50.1"),
        &client,
    )
    .await
    .expect("exact");
    assert_eq!(
        page.artifacts[0].source_ref.as_deref(),
        Some("commit-of-core-v2.50.1")
    );
    assert_eq!(
        page.artifacts[0].release_tag.as_deref(),
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
async fn release_tags_are_checked_for_each_lookup() {
    let http = RouteHttp::routes(vec![
        ("/pypi/requests/json", pypi_project()),
        ("/pypi/requests/2.31.0/json", pypi_release("2.31.0")),
    ]);
    let tags = KnownTags::new(vec!["psf/requests@v2.31.0"]);
    let b = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let query = versioned(ArtifactType::Pypi, "requests", "2.31.0");
    for _ in 0..2 {
        let page = pypi(&query, &client).await.expect("exact");
        assert_eq!(
            page.artifacts[0].source_ref.as_deref(),
            Some("commit-of-v2.31.0")
        );
        assert_eq!(page.artifacts[0].release_tag.as_deref(), Some("v2.31.0"));
        assert!(page.artifacts[0].source_tag);
    }
    assert_eq!(
        tags.seen(),
        [
            "psf/requests@v2.31.0",
            "psf/requests@2.31.0",
            "psf/requests@v2.31.0",
            "psf/requests@2.31.0"
        ],
        "one round per lookup"
    );
}

/// A monorepo package names its directory in the project link
/// (`google-cloud-storage` is `packages/google-cloud-storage` of
/// `google-cloud-python`): the row splits the link into the repository and
/// its directory, whether the link is labeled or only the homepage.
#[tokio::test]
async fn a_project_link_into_a_monorepo_directory_names_the_repository_and_the_directory() {
    let monorepo =
        "https://github.com/googleapis/google-cloud-python/tree/main/packages/google-cloud-storage";
    for info in [
        json!({"name": "google-cloud-storage", "version": "3.1.0",
            "home_page": monorepo, "project_urls": {"Homepage": monorepo}}),
        json!({"name": "google-cloud-storage", "version": "3.1.0",
            "project_urls": {"Source": monorepo}}),
    ] {
        let http = RouteHttp::routes(vec![(
            "/pypi/google-cloud-storage/3.1.0/json",
            json!({"info": info, "urls": []}),
        )]);
        let tags = KnownTags::new(vec!["googleapis/google-cloud-python@main"]);
        let query = versioned(ArtifactType::Pypi, "google-cloud-storage", "3.1.0");
        let page = super::super::test_http::execute(&http, &query, Some(&tags))
            .await
            .expect("pypi");
        let row = &page.artifacts[0];
        assert_eq!(
            row.repository.as_deref(),
            Some("https://github.com/googleapis/google-cloud-python")
        );
        assert_eq!(
            row.repository_directory.as_deref(),
            Some("packages/google-cloud-storage")
        );
    }
}

/// A version PyPI does not have names which of its nearest releases are
/// yanked (every file yanked): published, and no release to move to.
#[tokio::test]
async fn a_missing_version_names_which_nearest_releases_are_yanked() {
    let http = RouteHttp::routes(vec![("/pypi/requests/json", pypi_project())]);
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let missing = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.31.9"),
        &client,
    )
    .await
    .expect_err("missing version");
    assert!(missing.missing_version, "{missing:?}");
    assert_eq!(missing.yanked, ["2.32.0"], "{missing:?}");
}

/// A release whose own metadata names no repository (requests 2.19.1 names
/// only its homepage) takes the one its project's latest release names, pins
/// its release tag there, and says where the repository came from. A
/// project whose latest release names none either keeps none, with one read
/// more.
#[tokio::test]
async fn a_release_naming_no_repository_takes_the_latest_release_s() {
    let mut old = pypi_release("2.19.1");
    old["info"]["project_urls"] = json!({"Homepage": "http://python-requests.org"});
    let http = RouteHttp::routes(vec![
        ("/pypi/requests/json", pypi_project()),
        ("/pypi/requests/2.19.1/json", old.clone()),
        ("/pypi/requests/2.32.3/json", pypi_release("2.32.3")),
    ]);
    let tags = KnownTags::new(vec!["psf/requests@v2.19.1=883caaf"]);
    let b = test_budget();
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let page = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.19.1"),
        &client,
    )
    .await
    .expect("exact");
    let row = &page.artifacts[0];
    assert_eq!(
        row.repository.as_deref(),
        Some("https://github.com/psf/requests")
    );
    assert_eq!(row.source_ref.as_deref(), Some("883caaf"));
    assert_eq!(row.release_tag.as_deref(), Some("v2.19.1"));
    assert!(row.source_tag);
    assert!(
        row.disclosures
            .iter()
            .any(|said| said.starts_with("requests 2.19.1:")
                && said.contains("latest release, 2.32.3")),
        "{:?}",
        row.disclosures
    );
    // The latest release itself names its own repository: nothing more is read.
    let latest = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.32.3"),
        &client,
    )
    .await
    .expect("latest");
    assert!(latest.artifacts[0].disclosures.is_empty());
    let mut bare = pypi_project();
    bare["info"]["project_urls"] = json!({});
    let http = RouteHttp::routes(vec![
        ("/pypi/requests/json", bare),
        ("/pypi/requests/2.19.1/json", old),
    ]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let page = pypi(
        &versioned(ArtifactType::Pypi, "requests", "2.19.1"),
        &client,
    )
    .await
    .expect("exact");
    assert_eq!(page.artifacts[0].repository, None);
    assert!(page.artifacts[0].disclosures.is_empty());
    assert_eq!(
        http.targets()
            .iter()
            .filter(|path| path.starts_with("/pypi/"))
            .count(),
        2
    );
}
