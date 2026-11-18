use super::*;
use octocode_test_support::json_fixture;

/// Two rows answered while the project loaded and one after: the note is
/// said once (R2), both flagged rows keep their reason and rerun, and no
/// response says what the reason means (R1).
#[test]
fn the_still_loading_note_is_said_once_without_the_reason_meaning() {
    let query: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
                "operation": "references",
                "path": "/w/a.rs",
                "symbolName": "f",
                "line": 1
            }"#,
    ))
    .expect("query");
    let rows = [Some(Readiness::Timeout), Some(Readiness::Timeout), None]
        .into_iter()
        .map(|readiness| {
            let mut row = json_fixture(
                r#"{
                        "files": [{"path": "/w/a.rs", "matches": []}],
                        "matchCount": 0
                    }"#,
            );
            flag_still_loading(&mut row, &query, readiness);
            row
        })
        .collect::<Vec<_>>();
    let response = octocode_runtime_base::test_support::staged_response(
        octocode_runtime_base::id::ToolId::LspSearch,
        rows,
    );
    let text = response.to_string();
    assert_eq!(
        text.matches("still loading the project").count(),
        1,
        "{response}"
    );
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&response);
    for index in 0..2 {
        assert_eq!(
            response["results"][index]["data"]["partialReasons"],
            json_fixture(r#"["languageServerIndexing"]"#),
            "{response}"
        );
    }
}
