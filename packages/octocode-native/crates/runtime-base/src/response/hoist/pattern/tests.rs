use super::*;
use serde_json::json;

fn pointers(pattern: &str, value: &Value) -> Vec<String> {
    Pattern::parse(pattern)
        .matches(value)
        .iter()
        .map(Match::pointer)
        .collect()
}

/// `*` reaches every item, in document order, and binds its index.
#[test]
fn a_star_reaches_every_item_and_binds_its_index() {
    let data = json!({"files": [
        {"path": "a", "matches": [{"line": 1}, {"line": 2}]},
        {"path": "b", "matches": [{"line": 3}]},
    ]});
    assert_eq!(
        pointers("/files/*/matches/*/line", &data),
        [
            "/files/0/matches/0/line",
            "/files/0/matches/1/line",
            "/files/1/matches/0/line"
        ]
    );
    let found = Pattern::parse("/files/*/matches/*").matches(&data);
    assert_eq!(found[2].bound, ["1", "0"]);
    assert_eq!(
        Pattern::parse("/files/*/path").bind(&found[2].bound),
        Some(vec!["files".to_owned(), "1".to_owned(), "path".to_owned()])
    );
    assert_eq!(pointers("", &data), [""]);
    assert!(pointers("/files/*/missing", &data).is_empty());
}

/// No pattern enters a call a reader runs, or a row's `next` and `related`
/// channels, even where the names would match.
#[test]
fn a_pattern_never_enters_a_call_or_a_call_channel() {
    let data = json!({
        "next": {"nextPage": {"owner": "o"}},
        "related": [{"owner": "o"}],
        "lead": {"tool": "localFetch", "input": {"owner": "o"}, "owner": "o"},
        "owner": "o",
    });
    assert!(pointers("/next/nextPage/owner", &data).is_empty());
    assert!(pointers("/related/*/owner", &data).is_empty());
    assert!(pointers("/lead/owner", &data).is_empty());
    assert_eq!(pointers("/owner", &data), ["/owner"]);
}

/// Tokens are RFC 6901: `~1` is `/` and `~0` is `~`.
#[test]
fn tokens_unescape_as_rfc_6901_spells_them() {
    let data = json!({"a/b": {"c~d": 1}});
    assert_eq!(pointers("/a~1b/c~0d", &data), ["/a~1b/c~0d"]);
}

/// On a record (an object keyed by data, such as `metavarRanges`), `*`
/// reaches every member and binds its key.
#[test]
fn a_star_on_a_record_reaches_every_member_and_binds_its_key() {
    let data = json!({"matches": [{"line": 4, "metavarRanges": {
        "$A": [{"line": 4, "column": 1}], "$B": [{"line": 5, "column": 2}],
    }}]});
    let found = Pattern::parse("/matches/*/metavarRanges/*/*/line").matches(&data);
    assert_eq!(
        found.iter().map(Match::pointer).collect::<Vec<_>>(),
        [
            "/matches/0/metavarRanges/$A/0/line",
            "/matches/0/metavarRanges/$B/0/line"
        ]
    );
    assert_eq!(found[1].bound, ["0", "$B", "0"]);
    assert_eq!(
        Pattern::parse("/matches/*/line").bind(&found[1].bound),
        Some(vec![
            "matches".to_owned(),
            "0".to_owned(),
            "line".to_owned()
        ])
    );
}
