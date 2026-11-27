use super::*;
use octocode_test_support::json_fixture;

#[test]
fn referenced_progress_is_private_and_only_accepted_from_an_issued_cursor() {
    let row = json_fixture(
        r#"{
            "path": "src/lib.rs",
            "readProgress": {"unit": "lines", "offset": 3}
        }"#,
    );
    assert!(state_fields(ToolId::LocalFetch).contains("readProgress"));
    assert!(crate::contracts::validate_query("localFetch", row.clone()).is_err());
    crate::contracts::validate_built_query("localFetch", row)
        .expect("the same progress is valid inside a saved request");
}
