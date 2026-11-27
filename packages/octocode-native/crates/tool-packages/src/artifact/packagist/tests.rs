use super::super::query::exact_query;
use super::super::test_http::RouteHttp;
use super::super::test_http::test_budget;
use super::*;

use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn composer_source_reference_is_the_release_commit() {
    let row = json_fixture(
        r#"{
            "name": "o/r",
            "version": "v1.0.0",
            "source": {
                "type": "git",
                "url": "https://github.com/o/r.git",
                "reference": "0a2e291c0d9c0c7675d445703e51750363a549ef"
            }
        }"#,
    );
    let item = composer(row.as_object().unwrap()).unwrap();
    assert_eq!(
        item.source_ref.as_deref(),
        Some("0a2e291c0d9c0c7675d445703e51750363a549ef")
    );
}

/// An exact Packagist row states the release day (`time`) and the
/// version's `require` entries as declared; a version entry without
/// `require` declares none.
#[tokio::test]
async fn packagist_exact_rows_state_the_release_day_and_requirements() {
    let entry = json_fixture(
        r#"{
            "name": "monolog/monolog",
            "version": "3.12.1",
            "time": "2026-09-29T19:21:12+00:00",
            "require": {"php": ">=8.1", "psr/log": "^2.0 || ^3.0"},
            "require-dev": {"phpunit/phpunit": "^10"}
        }"#,
    );
    let http = RouteHttp::always(json!({"packages": {"monolog/monolog": [entry]}}));
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let q = exact_query(ArtifactType::Packagist, "monolog/monolog");
    let state = ArtifactProviderState::default();
    let page = packagist(&q, &state, &client).await.expect("packagist");
    let item = &page.artifacts[0];
    assert_eq!(item.published_at.as_deref(), Some("2026-09-29"));
    assert_eq!(
        item.dependencies,
        Some(vec![
            "php >=8.1".to_owned(),
            "psr/log ^2.0 || ^3.0".to_owned()
        ])
    );
    let bare = RouteHttp::always(json_fixture(
        r#"{
            "packages": {
                "monolog/monolog": [{"name": "monolog/monolog", "version": "1.0.0"}]
            }
        }"#,
    ));
    let client = RegistryClient::new(&bare, &b);
    let page = packagist(&q, &state, &client).await.expect("packagist");
    assert_eq!(page.artifacts[0].dependencies, Some(Vec::new()));
    assert_eq!(page.artifacts[0].published_at, None);
}

/// The exact lookup of `o/r` against a p2 document with these entries.
async fn lookup(entries: Value) -> ArtifactProviderPage {
    let http = RouteHttp::always(json!({"minified": "composer/2.0", "packages": {"o/r": entries}}));
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    packagist(
        &exact_query(ArtifactType::Packagist, "o/r"),
        &ArtifactProviderState::default(),
        &client,
    )
    .await
    .expect("packagist")
}

/// Latest is what Composer's default `minimum-stability` installs: the
/// newest stable entry. A p2 document is minified (only the first entry is
/// complete; each later one states what changed, `__unset` removing a
/// field), so the row is that entry with every change before it applied.
#[tokio::test]
async fn packagist_latest_is_the_newest_stable_release_with_its_inherited_fields() {
    let sha = |c: &str| c.repeat(40);
    let page = lookup(json!([
        {"name": "o/r", "version": "v8.2.0-BETA1", "description": "kept", "license": ["MIT"],
         "time": "2026-11-01T00:00:00+00:00", "homepage": "https://h.example",
         "source": {"type": "git", "url": "https://github.com/o/r.git", "reference": sha("a")},
         "require": {"php": ">=8.4", "beta/only": "^1"}},
        {"version": "v8.1.11", "time": "2026-10-01T00:00:00+00:00",
         "source": {"type": "git", "url": "https://github.com/o/r.git", "reference": sha("b")},
         "require": {"php": ">=8.2"}, "homepage": "__unset"},
        {"version": "v8.1.10",
         "source": {"type": "git", "url": "https://github.com/o/r.git", "reference": sha("c")}}
    ]))
    .await;
    let item = &page.artifacts[0];
    assert_eq!(item.version.as_deref(), Some("v8.1.11"));
    assert_eq!(item.source_ref.as_deref(), Some(sha("b").as_str()));
    assert_eq!(item.published_at.as_deref(), Some("2026-10-01"));
    assert_eq!(item.dependencies, Some(vec!["php >=8.2".to_owned()]));
    // Inherited from the complete first entry, and removed by `__unset`.
    assert_eq!(item.description.as_deref(), Some("kept"));
    assert_eq!(item.license.as_deref(), Some("MIT"));
    assert_eq!(item.homepage, None);
    // A package with no stable entry names its newest one.
    let only = lookup(json_fixture(
        r#"[
            {"name": "o/r", "version": "2.0.0-RC2"},
            {"version": "2.0.0-beta1"}
        ]"#,
    ))
    .await;
    assert_eq!(only.artifacts[0].version.as_deref(), Some("2.0.0-RC2"));
}

/// Stability as Composer's `VersionParser::parseStability` reads a version.
#[tokio::test]
async fn packagist_reads_stability_the_way_composer_does() {
    for (version, stable) in [
        ("v8.2.0-BETA1", false),
        ("1.0.0-beta", false),
        ("1.0.0beta2", false),
        ("1.0.0-b1", false),
        ("1.0.0-alpha.3", false),
        ("1.0.0a1", false),
        ("3.0.0-RC2", false),
        ("3.0.0.rc1", false),
        ("dev-main", false),
        ("2.x-dev", false),
        ("1.0.0-beta1-dev", false),
        ("1.0.0", true),
        ("v1.2.3", true),
        ("1.2.3.4", true),
        ("1.0.0-stable", true),
        ("1.0.0-patch1", true),
        ("1.0.0-p1", true),
        ("1.0.0-pl2", true),
        ("1.0.0+build.5", true),
    ] {
        let page = lookup(json!([
            {"name": "o/r", "version": version},
            {"version": "0.9.0"}
        ]))
        .await;
        let expected = if stable { version } else { "0.9.0" };
        assert_eq!(
            page.artifacts[0].version.as_deref(),
            Some(expected),
            "{version}"
        );
    }
}

/// `abandoned` is Composer's deprecation of a package, with its
/// replacement when the maintainer named one.
#[tokio::test]
async fn an_abandoned_package_states_it_and_its_replacement() {
    let replaced = lookup(json_fixture(
        r#"[
            {
                "name": "o/r",
                "version": "6.3.0",
                "abandoned": "symfony/mailer"
            }
        ]"#,
    ))
    .await;
    let deprecated = replaced.artifacts[0].deprecated.as_deref().unwrap_or("");
    assert!(deprecated.contains("symfony/mailer"), "{deprecated}");
    let bare = lookup(json_fixture(
        r#"[{"name": "o/r", "version": "6.3.0", "abandoned": true}]"#,
    ))
    .await;
    assert!(bare.artifacts[0].deprecated.is_some());
    let live = lookup(json_fixture(
        r#"[{"name": "o/r", "version": "6.3.0", "abandoned": false}]"#,
    ))
    .await;
    assert_eq!(live.artifacts[0].deprecated, None);
}

/// A release's recorded commit is checked against its release tag: the tag
/// naming that commit verifies it, and the tag is named apart.
#[tokio::test]
async fn a_packagist_commit_the_release_tag_names_is_tag_verified() {
    let sha = "b".repeat(40);
    let http = RouteHttp::always(json!({"minified": "composer/2.0", "packages": {"o/r": [
        {"name": "o/r", "version": "v8.1.11",
         "source": {"type": "git", "url": "https://github.com/o/r.git", "reference": sha}}
    ]}}));
    let b = test_budget();
    // The known tag names `sha`, written out: the fake reads `'static` names.
    let tags = super::super::test_http::KnownTags::new(vec![
        "o/r@v8.1.11=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    ]);
    let client = RegistryClient {
        tags: Some(&tags),
        ..RegistryClient::new(&http, &b)
    };
    let page = packagist(
        &exact_query(ArtifactType::Packagist, "o/r"),
        &ArtifactProviderState::default(),
        &client,
    )
    .await
    .expect("packagist");
    let item = &page.artifacts[0];
    assert_eq!(item.source_ref.as_deref(), Some(sha.as_str()));
    assert_eq!(item.release_tag.as_deref(), Some("v8.1.11"));
    assert!(item.source_tag);
}
