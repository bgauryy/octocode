//! A duration sent without its sign where only durations back from now are
//! taken runs signed, and the note says the form to send.
use super::*;

#[test]
fn an_unsigned_duration_runs_back_from_now() {
    let sent =
        json!({"queries": [{"operation": "files", "path": ".", "modifiedAt": {"from": "P1D"}}]});
    let ran = accepted("structureSearch", sent.clone());
    assert_eq!(ran["queries"][0]["modifiedAt"]["from"], "-P1D", "{ran}");
    let notes = normalized_input("structureSearch", sent).1;
    assert!(notes.iter().any(|note| note.contains("-P1D")), "{notes:?}");
}

#[test]
fn a_value_that_is_no_duration_stays_rejected() {
    for from in ["2026-10-01", "P", "PX", "-P1D1D"] {
        let sent =
            json!({"queries": [{"operation": "files", "path": ".", "modifiedAt": {"from": from}}]});
        assert!(
            prepare_rows(
                "structureSearch",
                normalized_input("structureSearch", sent).0
            )
            .map_or(true, |rows| rows.iter().any(Result::is_err)),
            "{from} should stay rejected"
        );
    }
}
