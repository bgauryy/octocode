//! Row shaping: GitHub coordinates beside the repository URL and repository homepages.
use super::{ArtifactItem, ArtifactType, drop_repository_homepage, name_github_repository};
use octocode_test_support::json_fixture;
use serde_json::json;

/// A GitHub repository URL names its owner and repo beside itself; any
/// other host, or no repository, adds nothing.
#[test]
fn github_rows_name_owner_and_repo_after_the_repository_url() {
    let mut row = json_fixture(
        r#"{
            "name": "ajv",
            "repository": "https://github.com/ajv-validator/ajv.git",
            "version": "8.0.0"
        }"#,
    );
    name_github_repository(&mut row);
    assert_eq!(
        row.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["name", "repository", "owner", "repo", "version"]
    );
    assert_eq!(row["owner"], "ajv-validator");
    assert_eq!(row["repo"], "ajv");
    let mut other = json_fixture(r#"{"name": "x", "repository": "https://gitlab.com/g/x"}"#);
    name_github_repository(&mut other);
    assert!(other.get("owner").is_none() && other.get("repo").is_none());
    let mut none = json_fixture(r#"{"name": "x"}"#);
    name_github_repository(&mut none);
    assert_eq!(none, json_fixture(r#"{"name": "x"}"#));
}

/// A repository URL that is exactly `https://github.com/{owner}/{repo}`
/// says nothing `owner` and `repo` do not: it is dropped (kept with
/// `details`). A URL that says more (a ref, a `.git` suffix) stays.
#[test]
fn a_repository_url_restating_owner_and_repo_is_dropped() {
    let mut row = json_fixture(
        r#"{
            "name": "joi",
            "repository": "https://github.com/hapijs/joi",
            "version": "1"
        }"#,
    );
    name_github_repository(&mut row);
    assert_eq!(
        row,
        json_fixture(
            r#"{
                "name": "joi",
                "owner": "hapijs",
                "repo": "joi",
                "version": "1"
            }"#
        )
    );
    assert_eq!(
        row.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["name", "owner", "repo", "version"]
    );
    let mut pinned = json_fixture(
        r#"{
            "name": "convict",
            "repository": "https://github.com/mozilla/node-convict.git#master"
        }"#,
    );
    name_github_repository(&mut pinned);
    assert_eq!(
        pinned["repository"],
        "https://github.com/mozilla/node-convict.git#master"
    );
    assert_eq!(
        (&pinned["owner"], &pinned["repo"]),
        (&json!("mozilla"), &json!("node-convict"))
    );
}

fn row(homepage: &str, repository: &str) -> ArtifactItem {
    let mut item = ArtifactItem::new(ArtifactType::Npm, "x".into(), "https://r/x".into());
    item.homepage = Some(homepage.into());
    item.repository = Some(repository.into());
    item.description = Some("d".into());
    item
}

#[test]
fn only_a_homepage_that_is_the_repository_page_is_dropped() {
    let mut same = row("https://github.com/o/x#readme", "https://github.com/o/x");
    drop_repository_homepage(&mut same);
    assert_eq!(same.homepage, None);
    // A tree page of the repository, or a `.git` repository URL, is the
    // repository too.
    for (homepage, repository) in [
        (
            "https://github.com/DT/DT/tree/master/types/node",
            "https://github.com/DT/DT",
        ),
        (
            "https://github.com/Seldaek/monolog",
            "https://github.com/Seldaek/monolog.git",
        ),
    ] {
        let mut same = row(homepage, repository);
        drop_repository_homepage(&mut same);
        assert_eq!(same.homepage, None, "{homepage}");
    }
    let mut distinct = row("https://zod.dev", "https://github.com/o/x");
    drop_repository_homepage(&mut distinct);
    assert_eq!(distinct.homepage.as_deref(), Some("https://zod.dev"));
    // The registry URL is verbose: the verbose stage drops it by default.
}

#[test]
fn rows_carry_no_type_and_start_with_the_name() {
    let encoded =
        serde_json::to_value(row("https://zod.dev", "https://github.com/o/x")).expect("row");
    let keys = encoded.as_object().expect("row").keys().collect::<Vec<_>>();
    assert_eq!(
        keys.first().map(|key| key.as_str()),
        Some("packageName"),
        "{encoded}"
    );
    assert!(!keys.iter().any(|key| *key == "type"), "{encoded}");
}

/// A package with no stable release says so: the version it names is a
/// prerelease because nothing else exists, not because one was chosen.
#[tokio::test]
async fn a_package_with_only_prereleases_warns() {
    use crate::artifact::test_http::{RouteHttp, test_budget};
    use crate::artifact::{ArtifactProviderContext, execute_artifact};
    use octocode_runtime_base::providers::saved_answers::ReusedSince;
    let row_for = |version: &'static str| async move {
        let http = RouteHttp::always(json!({"minified": "composer/2.0", "packages": {
            "o/r": [{"name": "o/r", "version": version}]
        }}));
        let budget = test_budget();
        let reused = ReusedSince::new();
        let query = super::typed(&json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "packagist",
                "packageName": "o/r"
            }"#,
        ));
        let page = execute_artifact(
            &query,
            &ArtifactProviderContext {
                http: &http,
                budget: &budget,
                npm_registry: None,
                allow_private_registry: false,
                answers: None,
                reused: &reused,
                tags: None,
                share: None,
            },
        )
        .await
        .expect("packagist");
        super::respond(&query, page).expect("row")
    };
    let only = row_for("2.0.0-RC2").await;
    assert!(
        only["warnings"].to_string().contains("no stable release"),
        "{only}"
    );
    let stable = row_for("2.0.0").await;
    assert!(
        !stable["warnings"].to_string().contains("no stable release"),
        "{stable}"
    );
}
