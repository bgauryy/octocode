//! A server that says it is not working: what it answers may be incomplete or
//! wrong, so a row that reads as settled says so, in the server's words.

use super::*;

fn reasons(row: &Value) -> Vec<&str> {
    row["partialReasons"]
        .as_array()
        .map(|listed| listed.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn warned(row: &Value, words: &str) -> bool {
    row["warnings"].as_array().is_some_and(|listed| {
        listed
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains(words))
    })
}

/// A project that failed to load answers empty: not a settled "no uses".
#[test]
fn an_empty_answer_from_a_server_that_failed_to_load_is_partial_with_its_message() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("healthError", "rust", Duration::ZERO);
    let row = server.definition();
    assert_eq!(row["status"], "empty", "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(reasons(&row).contains(&"serverUnhealthy"), "{row}");
    assert!(warned(&row, "unparsable Cargo.toml"), "{row}");
}

/// The same on the operation an agent reads as "no uses", and its coverage
/// does not call that complete.
#[test]
fn references_from_a_server_that_failed_to_load_are_not_complete() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("healthError", "rust", Duration::ZERO);
    let row = server.references();
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(reasons(&row).contains(&"serverUnhealthy"), "{row}");
    assert_ne!(
        row.pointer("/coverage/state"),
        Some(&json!("complete")),
        "{row}"
    );
}

/// A partly working server's answers are flagged too: "some results might be
/// wrong".
#[test]
fn a_warning_makes_the_row_partial_with_its_message() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("healthWarning", "rust", Duration::ZERO);
    let row = server.definition();
    assert!(reasons(&row).contains(&"serverUnhealthy"), "{row}");
    assert!(warned(&row, "build scripts"), "{row}");
}

/// A server need not say why: the row still names what it reported.
#[test]
fn a_status_without_a_message_still_flags_the_row() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("healthUnexplained", "rust", Duration::ZERO);
    let row = server.definition();
    assert!(reasons(&row).contains(&"serverUnhealthy"), "{row}");
    assert!(warned(&row, "error"), "{row}");
}

/// The last status decides: the manifest was fixed and the project reloaded.
#[test]
fn a_recovered_server_clears_the_flag() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("healthRecovers", "rust", Duration::ZERO);
    assert!(reasons(&server.definition()).contains(&"serverUnhealthy"));
    let row = server.definition();
    assert!(!reasons(&row).contains(&"serverUnhealthy"), "{row}");
}

/// A server that never reports health claims nothing about it.
#[test]
fn a_server_without_health_reports_is_not_flagged() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("silent", "go", Duration::from_millis(300));
    assert!(!reasons(&server.references()).contains(&"serverUnhealthy"));
}
