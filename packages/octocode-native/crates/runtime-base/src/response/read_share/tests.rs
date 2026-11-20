use super::*;
#[test]
fn every_row_receives_a_positive_share_and_numbering_counts_utf16() {
    for rows in [1, 3, 20] {
        for budget in [1, 500, 20000] {
            assert!(row_window(ToolId::LocalFetch, budget, rows) > 0);
        }
    }
    assert_eq!(
        text_chars("😀\n", 1),
        octocode_engine::json_size::string_body_units("😀\n") + 3
    );
}

fn filled(chars: usize, open: bool) -> Filled {
    Filled { chars, open }
}

/// A row that finishes under its share leaves the rest of the room to the
/// rows still reading, split evenly among them.
#[test]
fn finished_rows_leave_their_unused_share_to_the_rows_still_reading() {
    let (room, share) = (9_000, 3_000);
    let rows = [filled(600, false), filled(2_990, true), filled(2_980, true)];
    assert_eq!(widened(room, share, &rows), Some((room - 600) / 2));
    let rows = [filled(600, false), filled(400, false), filled(2_990, true)];
    assert_eq!(widened(room, share, &rows), Some(room - 1_000));
}

/// Water stops rising once no row still reading would gain: every row
/// finished, every row still reads at an even share, or a settled row that
/// cannot be cut (one wide line) already takes more than its share.
#[test]
fn the_fill_ends_when_no_row_still_reading_would_gain() {
    let (room, share) = (9_000, 3_000);
    assert_eq!(
        widened(room, share, &[filled(600, false), filled(900, false)]),
        None
    );
    assert_eq!(
        widened(
            room,
            share,
            &[
                filled(2_990, true),
                filled(2_990, true),
                filled(2_990, true)
            ]
        ),
        None
    );
    assert_eq!(
        widened(room, share, &[filled(7_000, false), filled(2_990, true)]),
        None
    );
    assert_eq!(widened(room, share, &[]), None);
}

/// Exactly the source rows whose re-read sends nothing fill the page
/// together (localFetch, and ghGetFileContent at its resolved commit); a
/// judged row is never read again, and a row reads on only while its own
/// walk continues.
#[test]
fn only_unjudged_source_rows_read_again_for_free_fill_together() {
    for tool in ToolId::ALL {
        assert_eq!(
            fill_together(tool),
            tool.reads_source() && reread_is_free(tool),
            "{tool}"
        );
    }
    assert!(fill_together(ToolId::LocalFetch));
    assert!(fill_together(ToolId::GhGetFileContent));
    assert!(reads_again(&json!({"path": "a.rs"})));
    for field in crate::contracts::PROVIDER_FIELDS {
        let mut judged = json!({"path": "a.rs"});
        judged[field] = json!({});
        assert!(!reads_again(&judged), "{field}");
    }
    assert!(reads_on(
        &json!({"next": {"nextPage": {"tool": "localFetch"}}})
    ));
    assert!(!reads_on(
        &json!({"next": {"read": {"tool": "localFetch"}}})
    ));
    assert!(!reads_on(&json!({"content": "1\n"})));
}
