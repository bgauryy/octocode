use super::place_numbered;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn each_lead_takes_the_next_free_key_and_keeps_the_others() {
    let mut row = json_fixture(r#"{"calls": {"broadenSearch2": "held", "other": 1}}"#);
    for lead in ["a", "b", "c"] {
        place_numbered(&mut row, "broadenSearch", json!(lead));
    }
    assert_eq!(
        row["calls"],
        json_fixture(
            r#"{
                "broadenSearch": "a",
                "broadenSearch2": "held",
                "broadenSearch3": "b",
                "broadenSearch4": "c",
                "other": 1
            }"#
        )
    );
}

#[test]
fn a_row_without_calls_gets_them() {
    let mut row = json!({});
    place_numbered(&mut row, "broadenSearch", json!("a"));
    assert_eq!(row["calls"]["broadenSearch"], "a");
}
