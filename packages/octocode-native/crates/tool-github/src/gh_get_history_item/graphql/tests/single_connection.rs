use super::*;
use octocode_test_support::json_fixture;

#[test]
fn section_headers_do_not_depend_on_unselected_labels() {
    let mut raw = json_fixture(
        r#"{
            "number": 7,
            "state": "OPEN",
            "headRefOid": "2222222222222222222222222222222222222222",
            "baseRefOid": "1111111111111111111111111111111111111111",
            "labels": {
                "pageInfo": {"hasNextPage": true},
                "nodes": [{"name": "bug"}]
            }
        }"#,
    );
    let query = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 7,
            "sections": ["reviews"]
        }"#,
    ))
    .expect("valid section read");
    let header = |raw: &Value| {
        crate::gh_get_history_item::pr_finish::pr_metadata(&map_graphql_pr_metadata(raw), &query)
    };
    let with_labels = header(&raw);
    raw.as_object_mut().unwrap().remove("labels");
    assert_eq!(header(&raw), with_labels);
    assert!(with_labels.get("labels").is_none());
    assert!(with_labels.get("labelsClipped").is_none());
}

#[test]
fn graphql_file_metadata_requires_a_lossless_inventory_representation() {
    let file = |kind| {
        json!({"files":{"nodes":[{
            "path":"src/a.rs", "additions":1, "deletions":0, "changeType":kind
        }]}})
    };
    for kind in ["ADDED", "DELETED", "MODIFIED"] {
        assert!(files_have_complete_metadata(&file(kind)), "{kind}");
    }
    for kind in ["RENAMED", "COPIED", "UNKNOWN"] {
        assert!(!files_have_complete_metadata(&file(kind)), "{kind}");
    }
    for field in ["path", "additions", "deletions", "changeType"] {
        let mut incomplete = file("MODIFIED");
        incomplete["files"]["nodes"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(!files_have_complete_metadata(&incomplete), "{field}");
    }
    assert!(files_have_complete_metadata(&json_fixture(
        r#"{"files": {"nodes": []}}"#
    )));
    assert!(!files_have_complete_metadata(&json!({})));
}

#[test]
fn a_single_connection_uses_graphql_but_body_only_and_filtered_files_do_not() {
    for section in ["files", "comments", "reviews", "commits"] {
        let row = json!({
            "operation":"pullRequest", "owner":"a", "repo":"b", "number":7,
            "sections":[section]
        });
        let request = HistoryItemRequest::from_row(row.clone()).expect("valid connection read");
        assert!(graphql_complete_collection_eligible(&request), "{section}");
        let mut later = row;
        later["page"] = json!(2);
        let request = HistoryItemRequest::from_row(later).expect("valid later page");
        assert!(!graphql_complete_collection_eligible(&request), "{section}");
    }
    for row in [
        json_fixture(r#"{"sections": ["body"]}"#),
        json_fixture(r#"{"sections": ["files"], "include": ["src/**"]}"#),
        json_fixture(r#"{"sections": ["body", "files"], "include": ["src/**"]}"#),
        json_fixture(r#"{"sections": ["files", "patches"]}"#),
    ] {
        let mut request = json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7
            }"#,
        );
        request
            .as_object_mut()
            .unwrap()
            .extend(row.as_object().unwrap().clone());
        let request = HistoryItemRequest::from_row(request).expect("valid non-GraphQL read");
        assert!(!graphql_complete_collection_eligible(&request));
    }
}
