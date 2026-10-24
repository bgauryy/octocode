//! PEP 440 specifiers against python-dateutil's real release list
//! (pypi.org, 2026-10-24), with 2.8.2 marked yanked for the yanked policy.
use super::super::query::versioned;
use super::super::test_http::{RouteHttp, test_budget};
use super::*;

use serde_json::json;

/// python-dateutil's releases and how many files each has.
const RELEASES: &str = "0.1:0 0.3:0 0.4:0 0.5:0 1.0:0 1.1:0 1.2:0 1.4:1 1.4.1:1 1.5:1 2.0:0 \
     2.1:1 2.2:1 2.3:2 2.4.0:2 2.4.1:4 2.4.2:2 2.5.0:3 2.5.1:3 2.5.2:3 2.5.3:3 2.6.0:3 2.6.1:2 \
     2.7.0:2 2.7.1:2 2.7.2:2 2.7.3:2 2.7.4:2 2.7.5:2 2.8.0:2 2.8.1:2 2.8.2:2 2.9.0:2 \
     2.9.0.post0:2 3.0.0:2";
const YANKED: &str = "2.8.2";

fn release(version: &str) -> serde_json::Value {
    json!({
        "info": {"name": "python-dateutil", "version": version, "summary": "Extensions to datetime",
            "yanked": version == YANKED},
        "urls": [{"upload_time_iso_8601": "2024-03-01T00:00:00Z"}]
    })
}

fn http() -> RouteHttp {
    let mut project = release("3.0.0");
    let mut releases = serde_json::Map::new();
    let mut routes = Vec::new();
    for entry in RELEASES.split_whitespace() {
        let (version, files) = entry.split_once(':').expect("version:files");
        let files = files.parse::<usize>().expect("file count");
        releases.insert(
            version.to_owned(),
            json!(vec![json!({"yanked": version == YANKED}); files]),
        );
        routes.push((
            format!("/pypi/python-dateutil/{version}/json"),
            release(version),
        ));
    }
    project["releases"] = serde_json::Value::Object(releases);
    routes.push(("/pypi/python-dateutil/json".to_owned(), project));
    RouteHttp::routes(
        routes
            .iter()
            .map(|(target, body)| (target.as_str(), body.clone()))
            .collect(),
    )
}

async fn lookup(version: &str) -> Result<ArtifactProviderPage, ArtifactError> {
    let http = http();
    let budget = test_budget();
    let client = RegistryClient::uncached(&http, &budget);
    pypi(
        &versioned(ArtifactType::Pypi, "python-dateutil", version),
        &client,
    )
    .await
}

/// The three live reproductions: a post release is a release, and it
/// orders after its base release (2.9.0 < 2.9.0.post0 < 3.0.0).
#[tokio::test]
async fn post_releases_match_and_order_after_their_base_release() {
    for version in [
        "2.9.0.post0",
        ">=2.9.0.post0,<3",
        ">=2.8,<3",
        "~=2.9",
        "==2.9.*",
        ">2.8.2,<3",
    ] {
        let page = lookup(version)
            .await
            .unwrap_or_else(|error| panic!("{version}: {error:?}"));
        assert_eq!(
            page.artifacts[0].version.as_deref(),
            Some("2.9.0.post0"),
            "{version}"
        );
    }
    // `>V` admits no post release of V (PEP 440): 2.9.0.post0 is not >2.9.0.
    let excluded = lookup(">2.9.0,<3")
        .await
        .expect_err("only 2.9.0.post0 is above");
    assert_eq!(excluded.code, ErrorCode::VersionNotFound);
}

/// A specifier only a yanked release matches names it; a pin selects it.
#[tokio::test]
async fn yanked_releases_are_named_never_resolved() {
    let error = lookup(">=2.8.2,<2.9")
        .await
        .expect_err("only 2.8.2 matches");
    assert_eq!(error.code, ErrorCode::VersionNotFound);
    assert!(error.message.contains("yanked 2.8.2"), "{error:?}");
    assert_eq!(error.nearest, ["2.8.2"]);
    let page = lookup(">=2.8,<2.9").await.expect("2.8.1 is not yanked");
    assert_eq!(page.artifacts[0].version.as_deref(), Some("2.8.1"));
    let pinned = lookup("==2.8.2")
        .await
        .expect("a pin selects a yanked release");
    assert_eq!(pinned.artifacts[0].yanked, Some(true));
}

/// A valid specifier that matches nothing is `versionNotFound`; one that
/// is not PEP 440 is invalid input, never a missing version.
#[tokio::test]
async fn invalid_specifiers_are_invalid_input_not_missing_versions() {
    let missing = lookup(">=4").await.expect_err("no 4.x release");
    assert_eq!(missing.code, ErrorCode::VersionNotFound);
    for invalid in [
        ">=2.8,<<3",
        "~=2",
        ">=2.*",
        "==2.9.0a1.*",
        ">2.9+local",
        "=>2",
    ] {
        let error = lookup(invalid)
            .await
            .expect_err(&format!("{invalid} is no specifier"));
        assert_eq!(error.code, ErrorCode::InvalidInput, "{invalid}: {error:?}");
        assert!(error.message.contains("PEP 440"), "{invalid}: {error:?}");
    }
}
