use super::*;
use octocode_test_support::json_fixture;
use octocode_tools_shared::gh_shared::query::parse;
use serde_json::json;
#[test]
fn typed_repository_filters_preserve_units_bounds_and_or_locations() {
    let q = repositories(&parse(json_fixture(
        r#"{
            "operation": "search",
            "owner": "o",
            "matchIn": ["name", "readme"],
            "filters": {
                "archive": "only",
                "stars": {"min": 0, "max": 10},
                "sizeKiB": {"max": 5000},
                "visibility": "public",
                "language": "Common Lisp",
                "topics": ["machine-learning"],
                "isSponsorable": true,
                "createdAt": {"from": "2024-01-01T00:00:00Z"}
            }
        }"#,
    )));
    for term in [
        "user:o",
        "fork:true",
        "archived:true",
        "stars:0..10",
        "size:<=5000",
        "is:public",
        "language:\"Common Lisp\"",
        "topic:machine-learning",
        "is:sponsorable",
        "created:>=2024-01-01T00:00:00Z",
        "in:name,readme",
    ] {
        assert!(q.contains(term), "{term}: {q}");
    }
    assert!(!q.contains("archived:false"));
}
#[test]
fn archive_default_and_inclusion_are_distinct() {
    for (archive, expected) in [
        (None, Some("archived:false")),
        (Some("include"), None),
        (Some("only"), Some("archived:true")),
    ] {
        let mut row = json_fixture(r#"{"operation": "search", "keywords": ["client"]}"#);
        if let Some(archive) = archive {
            row["filters"] = json!({"archive":archive});
        }
        let q = repositories(&parse(row));
        assert_eq!(
            q.split_whitespace()
                .find(|part| part.starts_with("archived:")),
            expected
        );
    }
}
