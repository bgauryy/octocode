//! Query fields beside a cursor, read against the saved query it restores.
use super::*;

fn against(tool: &str, sent: &Value, saved: &Value) -> (Value, BesideCursor) {
    let mut input = sent.clone();
    let read = read_beside_cursor(tool, &mut input, saved);
    (input, read)
}

fn held_notes(read: BesideCursor) -> Vec<String> {
    match read {
        BesideCursor::Held(notes) => notes,
        other => panic!("not held: {other:?}"),
    }
}

/// The first field of `input` beside its cursor: the one validation names
/// first.
fn leading(input: &Value) -> Option<&str> {
    input
        .as_object()?
        .keys()
        .map(String::as_str)
        .find(|field| *field != "cursor")
}

/// A field at the value every saved row holds is dropped with one note
/// naming it, and the cursor alone validates.
#[test]
fn a_field_at_its_saved_value_is_dropped_with_one_note() {
    let saved = json!({"queries": [{"operation": "search", "keywords": ["x"], "limit": 3}]});
    for (sent, named) in [
        (
            json!({"cursor": "issued", "operation": "search"}),
            vec!["`operation`"],
        ),
        (
            json!({"cursor": "issued", "keywords": ["x"]}),
            vec!["`keywords`"],
        ),
        (
            json!({"cursor": "issued", "operation": "search", "keywords": ["x"], "limit": 3}),
            vec!["`operation`", "`keywords`", "`limit`"],
        ),
    ] {
        let (input, read) = against("ghSearchRepo", &sent, &saved);
        assert_eq!(input, json!({"cursor": "issued"}), "{sent}");
        let notes = held_notes(read);
        assert!(
            matches!(notes.as_slice(), [note]
                if named.iter().all(|field| note.contains(field))
                    && note.contains(r#"{"cursor":"…"} alone"#)),
            "{sent}: {notes:?}"
        );
        crate::contracts::validate("ghSearchRepo", input).expect("the cursor alone is valid");
    }
    let saved = json!({"queries": [{"path": "/work/src", "matchString": ["needle"]}]});
    let sent = json!({"cursor": "issued", "path": "/work/src"});
    let (input, read) = against("localSearch", &sent, &saved);
    assert_eq!(input, json!({"cursor": "issued"}));
    assert_eq!(held_notes(read).len(), 1);
}

/// A field at another value, or one a saved row lacks or the rows disagree
/// on, leaves the call for validation to refuse, led by that field; the
/// values the saved query holds are named for the refusal, and a field
/// beside it at its saved value is dropped and named as one to drop.
#[test]
fn a_field_at_another_value_stays_as_sent_and_names_the_saved_value() {
    let saved = json!({"queries": [{"operation": "search", "keywords": ["x"]}]});
    let sent = json!({"cursor": "issued", "operation": "list"});
    let (input, read) = against("ghSearchRepo", &sent, &saved);
    assert_eq!(input, sent);
    assert!(
        matches!(&read, BesideCursor::Holds(holds) if holds.contains(r#"`operation: "search"`"#)),
        "{read:?}"
    );
    crate::contracts::validate("ghSearchRepo", input).expect_err("refused");
    // An equal field beside a differing one does not run: the differing one
    // leads the refusal, and the equal one is named as one to drop.
    let sent = json!({"cursor": "issued", "operation": "search", "keywords": ["y"]});
    let (input, read) = against("ghSearchRepo", &sent, &saved);
    assert_eq!(input, json!({"cursor": "issued", "keywords": ["y"]}));
    assert_eq!(leading(&input), Some("keywords"));
    assert!(
        matches!(&read, BesideCursor::Holds(holds)
            if holds.contains(r#"`keywords: ["x"]`"#)
                && holds.contains("`operation`")
                && !holds.contains("`operation:")),
        "{read:?}"
    );
    crate::contracts::validate("ghSearchRepo", input).expect_err("refused");
    // Nothing to name: the saved query holds no `owner`, or its rows disagree.
    let sent = json!({"cursor": "issued", "owner": "o"});
    assert_eq!(
        against("ghSearchRepo", &sent, &saved),
        (sent, BesideCursor::AsSent)
    );
    // A field the saved query does not hold leads, whatever was sent first.
    let sent = json!({"cursor": "issued", "operation": "search", "owner": "o"});
    let (input, read) = against("ghSearchRepo", &sent, &saved);
    assert_eq!(input, json!({"cursor": "issued", "owner": "o"}));
    assert_eq!(leading(&input), Some("owner"));
    assert!(
        matches!(&read, BesideCursor::Holds(holds)
            if holds.contains("`operation`") && !holds.contains("owner")),
        "{read:?}"
    );
    let rows = json!({"queries": [
        {"operation": "search", "keywords": ["x"]},
        {"operation": "search", "keywords": ["y"]}
    ]});
    let sent = json!({"cursor": "issued", "keywords": ["x"]});
    assert_eq!(
        against("ghSearchRepo", &sent, &rows),
        (sent, BesideCursor::AsSent)
    );
    let sent = json!({"cursor": "issued", "operation": "search"});
    let (input, read) = against("ghSearchRepo", &sent, &rows);
    assert_eq!(input, json!({"cursor": "issued"}));
    assert_eq!(held_notes(read).len(), 1, "every saved row holds it");
}

/// A private progress field is never read against the saved query, at its
/// saved value too, and nothing beside it is dropped; it leads the call
/// validation refuses.
#[test]
fn a_private_field_beside_a_cursor_is_never_compared() {
    for (tool, private) in [("ghSearchRepo", "page"), ("localSearch", "snapshot")] {
        let id = ToolId::from_name(tool).expect("a tool");
        assert!(
            crate::contracts::state_fields(id).contains(private),
            "{tool}"
        );
        let saved = json!({"queries": [{"operation": "search", "path": "/work", private: 2}]});
        for sent in [
            json!({"cursor": "issued", private: 2, "operation": "search"}),
            json!({"cursor": "issued", "operation": "search", private: 2}),
        ] {
            let (input, read) = against(tool, &sent, &saved);
            assert_eq!((&input, read), (&sent, BesideCursor::AsSent), "{tool}");
            assert_eq!(leading(&input), Some(private), "{tool}: {input}");
            crate::contracts::validate(tool, input).expect_err("refused");
        }
    }
}

/// A page size keeps its own reading ([`cursor_alone`]): alone it is not
/// this read's, and beside a held field it is dropped with its own note. A
/// request field or an unknown one is validation's to refuse, and leads it.
#[test]
fn a_page_size_keeps_its_note_and_other_request_fields_stay() {
    let saved = json!({"queries": [{"operation": "search"}], "responseChars": 900});
    let sent = json!({"cursor": "issued", "responseChars": 900});
    assert_eq!(
        against("ghSearchRepo", &sent, &saved),
        (sent, BesideCursor::AsSent)
    );
    let sent = json!({"cursor": "issued", "operation": "search", "responseChars": 5});
    let (input, read) = against("ghSearchRepo", &sent, &saved);
    assert_eq!(input, json!({"cursor": "issued"}));
    let notes = held_notes(read);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(
        notes.iter().any(|note| note.contains("`operation`")),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|note| note.contains("page size it was issued with")),
        "{notes:?}"
    );
    let sent = json!({"cursor": "issued", "queries": [{"operation": "search"}]});
    assert_eq!(
        against("ghSearchRepo", &sent, &saved),
        (sent, BesideCursor::AsSent)
    );
    let sent = json!({"cursor": "issued", "operation": "search", "unknownField": 1});
    let (input, read) = against("ghSearchRepo", &sent, &saved);
    assert_eq!(input, json!({"cursor": "issued", "unknownField": 1}));
    assert_eq!(leading(&input), Some("unknownField"));
    assert!(
        matches!(&read, BesideCursor::Holds(holds) if holds.contains("`operation`")),
        "{read:?}"
    );
}
