use super::{artifact_query, artifact_search_query};
use octocode_test_support::json_fixture;

#[test]
fn canonical_coordinate_and_version_stay_separate() {
    let exact = artifact_query(
        json_fixture(
            r#"{
                "operation": "resolve",
                "packageName": "@scope/package",
                "version": "^2.0"
            }"#,
        ),
        None,
    );
    assert_eq!(
        artifact_search_query::package_name(&exact),
        Some("@scope/package")
    );
    assert_eq!(artifact_search_query::version(&exact), Some("^2.0"));
    // Spaces around a version are no part of it.
    let padded = artifact_query(
        serde_json::json!({"operation": "resolve", "packageName": "lodash", "version": " 4.17.20 "}),
        None,
    );
    assert_eq!(artifact_search_query::version(&padded), Some("4.17.20"));
    for row in [
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "package@2.0"
            }"#,
        ),
        json_fixture(r#"{"ecosystem": "npm", "keywords": ["http"]}"#),
    ] {
        assert!(
            serde_json::from_value::<octocode_contract_types::ArtifactSearchQuery>(row).is_err()
        );
    }
}

/// A cursor names one page's start: moving the page clears it, and a
/// cursor of any length is carried.
#[test]
fn a_cursor_belongs_to_its_page() {
    let mut query = artifact_query(
        json_fixture(
            r#"{
                "operation": "search",
                "ecosystem": "go",
                "keywords": ["http"],
                "page": 2,
                "registryCursor": "c2"
            }"#,
        ),
        None,
    );
    assert_eq!(artifact_search_query::cursor(&query), Some("c2"));
    artifact_search_query::set_page(&mut query, 3);
    assert_eq!(
        (
            artifact_search_query::page(&query),
            artifact_search_query::cursor(&query)
        ),
        (3, None)
    );
    artifact_search_query::set_cursor(&mut query, Some("c3"));
    assert_eq!(artifact_search_query::cursor(&query), Some("c3"));
    // A registry cursor has no contract ceiling (X14): a long one is carried.
    let long = "x".repeat(20_000);
    artifact_search_query::set_cursor(&mut query, Some(&long));
    assert_eq!(artifact_search_query::cursor(&query), Some(long.as_str()));
    let exact = artifact_query(
        json_fixture(r#"{"operation": "resolve", "packageName": "ajv"}"#),
        None,
    );
    assert_eq!(artifact_search_query::cursor(&exact), None);
}
