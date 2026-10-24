use super::*;
use serde_json::json;

/// Members nest under the parent row they follow: no member repeats
/// `parent`/`parentLine`; each keeps its kind and export state. A member
/// whose parent is off the page stays top level
/// naming it, and a declaration without members is a plain object.
/// Flattening restores every flat fact.
#[test]
fn declaration_rows_nest_members_under_their_parent() {
    let flat = [
        json!({"name":"Server","kind":"impl","line":109,"endLine":891}),
        json!({"name":"connect","kind":"method","line":194,"endLine":204,"exported":true,"docStartLine":164,"parent":"Server","parentLine":109}),
        json!({"name":"ready","kind":"method","line":307,"endLine":310,"exported":true,"parent":"Server","parentLine":109}),
        json!({"name":"inner","kind":"function","line":308,"parent":"ready"}),
        json!({"name":"Client","kind":"impl","line":984,"endLine":990}),
        json!({"name":"open","kind":"method","line":985,"endLine":989,"parent":"Client"}),
        json!({"name":"orphan","kind":"method","line":2000,"parent":"Gone","parentLine":7}),
    ];
    let rows = declaration_rows(&flat);
    assert_eq!(
        json!(rows),
        json!([
            {"symbolName":"Server","kind":"impl","line":109,"endLine":891,
             "members":[
                {"symbolName":"connect","kind":"method","exported":true,"line":194,"endLine":204,"docStartLine":164},
                {"symbolName":"ready","kind":"method","exported":true,"line":307,"endLine":310,
                 "members":[{"symbolName":"inner","kind":"function","line":308}]}
             ]},
            {"symbolName":"Client","kind":"impl","line":984,"endLine":990,
             "members":[{"symbolName":"open","kind":"method","line":985,"endLine":989}]},
            {"symbolName":"orphan","kind":"method","line":2000,"parent":"Gone","parentLine":7}
        ])
    );
    let restored = flatten_members(&rows);
    assert_eq!(restored.len(), flat.len());
    for (row, original) in restored.iter().zip(&flat) {
        assert_eq!(row["symbolName"], original["name"], "{row}");
        for key in [
            "kind",
            "line",
            "endLine",
            "exported",
            "docStartLine",
            "parent",
        ] {
            assert_eq!(row.get(key), original.get(key), "{key}: {row}");
        }
    }
}

/// Canonical facts of flat internal rows, as [`flatten_members`]
/// restores them: public key names, keys sorted.
fn facts(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let mut fields: Vec<(String, Value)> = row
                .as_object()
                .expect("row")
                .iter()
                .map(|(key, value)| {
                    let key = match key.as_str() {
                        "name" => "symbolName",
                        "character" => "column",
                        other => other,
                    };
                    (key.to_owned(), value.clone())
                })
                .collect();
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(fields.into_iter().collect())
        })
        .collect()
}

/// Every outline row and every nested member is an object, whatever
/// facts it carries: a member-less declaration is never a string.
#[test]
fn every_row_and_member_is_an_object() {
    fn all_objects(rows: &[Value]) -> bool {
        rows.iter().all(|row| {
            row.is_object()
                && row
                    .get("members")
                    .and_then(Value::as_array)
                    .is_none_or(|members| all_objects(members))
        })
    }
    let flat = [
        json!({"name":"lone","kind":"function","line":1}),
        json!({"name":"Host","kind":"class","line":3,"endLine":9}),
        json!({"name":"get","kind":"method","line":4,"parent":"Host"}),
        json!({"name":"orphan","kind":"method","line":20,"parent":"Gone"}),
    ];
    let rows = declaration_rows(&flat);
    assert!(all_objects(&rows), "{rows:?}");
    assert_eq!(
        rows[0],
        json!({"symbolName":"lone","kind":"function","line":1})
    );
}

/// One span per top-level row: a container's span holds its members,
/// an orphan (its parent filtered out) is its own row, and a new file
/// starts a new row even when a name repeats across files.
#[test]
fn top_level_spans_cover_each_tree_once_and_split_files() {
    let flat = [
        json!({"name":"Host","kind":"class","line":3,"endLine":9,"path":"a.ts"}),
        json!({"name":"get","kind":"method","line":4,"parent":"Host","path":"a.ts"}),
        json!({"name":"inner","kind":"function","line":5,"parent":"get","path":"a.ts"}),
        json!({"name":"orphan","kind":"method","line":20,"parent":"Gone","path":"a.ts"}),
        json!({"name":"Host","kind":"class","line":1,"endLine":4,"path":"a.ts"}),
        json!({"name":"set","kind":"method","line":2,"parent":"Host","path":"b.ts"}),
    ];
    assert_eq!(top_level_spans(&flat), vec![0..3, 3..4, 4..5, 5..6]);
    // The spans tile the rows, so pages over spans reach each row once.
    let without_paths: Vec<Value> = flat[..4].to_vec();
    let spans = top_level_spans(&without_paths);
    assert_eq!(spans.len(), declaration_rows(&without_paths).len());
    assert!(top_level_spans(&[]).is_empty());
}

/// Round trip: every object row flattens back to its declaration's
/// facts, in source order, for awkward names and values, also after a
/// JSON round trip (the wire).
#[test]
fn object_rows_round_trip_every_fact_including_awkward_names() {
    let flat = [
        json!({"name":"operator()","kind":"method","line":3,"endLine":5,"exported":true}),
        json!({"name":"From<(A, B)> for X","kind":"impl","line":7,"endLine":20,"exported":true}),
        json!({"name":"from","kind":"function","line":8,"endLine":10,"docStartLine":7,"exported":true,
                "parent":"From<(A, B)> for X","parentLine":7}),
        json!({"name":"into (x)","kind":"function","line":11,"exported":true,"exportedAs":["a","b c","d (e)"],
                "character":4,"parent":"From<(A, B)> for X","parentLine":7}),
        json!({"name":"orphan","kind":"method","line":30,"parent":"Outer, Inc","parentLine":2}),
        json!({"name":"slice","kind":"method","line":33,"parent":"Foo for [T]","startLine":32}),
        json!({"name":"a, b","kind":"field","line":35}),
    ];
    let rows = declaration_rows(&flat);
    assert!(rows[1].get("shared").is_none());
    assert_eq!(
        rows[1]["members"][1],
        json!({"symbolName":"into (x)","kind":"function","exported":true,"line":11,"exportedAs":["a","b c","d (e)"],"column":4})
    );
    assert_eq!(facts(&flatten_members(&rows)), facts(&flat));
    let wire: Vec<Value> =
        serde_json::from_str(&serde_json::to_string(&rows).expect("json")).expect("parse");
    assert_eq!(facts(&flatten_members(&wire)), facts(&flat));
}
