//! A saved row's private progress is the runtime's own state, left as
//! written whatever it spells or however long it grows; the same text from
//! a caller, and the caller's own fields of a saved row, pass the gate every
//! caller's text passes.
use super::*;
use crate::id::ToolId;
use serde_json::json;

/// A fake GitHub token: `ghp_` and 36 alphanumerics.
const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

fn policy(tool: ToolId) -> InputPolicy {
    InputPolicy::of(tool)
}

/// Values of each kind private progress takes: an opaque token that spells
/// a credential (a selection plan, a provider's page cursor), a digest list
/// longer than a caller's string may be (the files a walk listed), and a
/// structure holding both (a read's progress).
fn states() -> [Value; 3] {
    let long = "0123456789abcdef".repeat(MAX_STRING_LENGTH / 16 + 1);
    [
        json!(TOKEN),
        json!(long),
        json!({"cursor": TOKEN, "listed": [long]}),
    ]
}

/// Every private field of every tool, holding each kind of value, stays as
/// written in a saved row, and the same value is refused from a caller.
#[test]
fn every_private_field_of_a_saved_row_is_kept_as_written() {
    let security = ContentSecurity::new();
    let mut checked_fields = 0;
    for tool in ToolId::ALL {
        for field in crate::contracts::state_fields(tool) {
            for value in states() {
                let row = json!({ field.as_str(): value.clone() });
                let saved = security
                    .check_saved_input(row.clone(), policy(tool), tool)
                    .unwrap_or_else(|issues| panic!("{tool:?}.{field}: {issues:?}"));
                assert_eq!(saved[field.as_str()], value, "{tool:?}.{field}");
                assert!(
                    security.check_input(row, policy(tool)).is_err(),
                    "{tool:?}.{field}: a caller's value is judged"
                );
            }
            checked_fields += 1;
        }
    }
    assert!(checked_fields > 0, "the contract declares private progress");
}

/// A saved row's caller fields keep the whole gate: a credential, or a
/// string past the caller's bound, is refused beside private progress.
#[test]
fn a_saved_rows_caller_fields_keep_the_whole_gate() {
    let security = ContentSecurity::new();
    let long = "a".repeat(MAX_STRING_LENGTH + 1);
    for (row, rule) in [
        (
            json!({"keywords": [TOKEN], "indexHead": TOKEN}),
            "security.credential",
        ),
        (
            json!({"keywords": [long], "indexHead": TOKEN}),
            "security.input",
        ),
    ] {
        let issues = security
            .check_saved_input(row.clone(), InputPolicy::Query, ToolId::GhSearchCode)
            .expect_err("a caller's field is judged");
        assert!(
            issues
                .iter()
                .all(|issue| issue.rule_id == rule && issue.path != ["indexHead"]),
            "{row}: {issues:?}"
        );
    }
}

/// Private progress is no caller's text: a response that repeats it has no
/// redaction undone for it, while the caller's own strings stay admitted.
#[test]
fn admitted_text_leaves_out_private_progress() {
    let saved = ContentSecurity::new()
        .check_saved_input(
            json!({"keywords": ["needle"], "indexHead": TOKEN}),
            InputPolicy::Query,
            ToolId::GhSearchCode,
        )
        .expect("admitted");
    let own = Admitted::of([&saved]);
    assert!(own.occurs_in("a needle here"));
    assert!(!own.occurs_in(TOKEN));
}
