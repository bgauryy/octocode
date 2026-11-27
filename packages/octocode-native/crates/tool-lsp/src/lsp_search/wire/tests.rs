use super::lean;
use octocode_test_support::json_fixture;
use octocode_tools_shared::same_line::mark;

#[test]
fn a_row_drops_the_column_and_the_path_the_request_states() {
    let mut row = json_fixture(
        r#"{
            "path": "a.rs",
            "workspaceRoot": "/w",
            "files": [
                {
                    "path": "a.rs",
                    "matches": [{"line": 3, "column": 7, "role": "call"}]
                }
            ]
        }"#,
    );
    lean(&mut row, Some("a.rs"));
    assert_eq!(
        row,
        json_fixture(
            r#"{
                "workspaceRoot": "/w",
                "files": [{"path": "a.rs", "matches": [{"line": 3, "role": "call"}]}]
            }"#
        )
    );
    // A path the request did not state, and a failed row's path, stay.
    let mut other = json_fixture(r#"{"path": "b.rs"}"#);
    lean(&mut other, Some("a.rs"));
    assert_eq!(other, json_fixture(r#"{"path": "b.rs"}"#));
    let mut failed = json_fixture(r#"{"path": "a.rs", "errorCode": "notFound"}"#);
    lean(&mut failed, Some("a.rs"));
    assert_eq!(failed["path"], "a.rs");
}

#[test]
fn a_column_stays_only_on_rows_the_page_marked_as_sharing_a_line() {
    let mut row = json_fixture(
        r#"{
            "files": [
                {
                    "path": "a.rs",
                    "matches": [
                        {"line": 4, "column": 11, "value": "f() + f()"},
                        {"line": 9, "column": 3, "value": "f()"}
                    ]
                }
            ]
        }"#,
    );
    // The row's twin on line 4 is on another page: only the mark says so.
    mark(&mut row["files"][0]["matches"][0]);
    lean(&mut row, None);
    assert_eq!(
        row["files"][0]["matches"],
        json_fixture(
            r#"[
                {"line": 4, "column": 11, "value": "f() + f()"},
                {"line": 9, "value": "f()"}
            ]"#
        )
    );
}
