use super::*;
use crate::contracts::prepare_rows;
use serde_json::json;

mod guessed_fields;

fn accepted(tool: &str, sent: Value) -> Value {
    let input = normalized_input(tool, sent.clone()).0;
    let rows = prepare_rows(tool, input.clone()).unwrap_or_else(|error| {
        panic!("{sent} should have one reading: {:?}", error.issues);
    });
    assert!(rows.iter().all(Result::is_ok), "{sent}: {rows:?}");
    input
}

#[test]
fn a_value_with_one_reading_runs() {
    let row = |input: &Value| input["queries"][0].clone();
    let one = row(&accepted(
        "localSearch",
        json!({"queries":[{"path":".","matchString":"x"}]}),
    ));
    assert_eq!(one["matchString"], json!(["x"]));
    let context = row(&accepted(
        "localSearch",
        json!({"queries":[{"path":".","matchString":["x"],"contextLines":2}]}),
    ));
    assert_eq!(context["contextLines"], json!({"before":2,"after":2}));
    let both = row(&accepted(
        "localSearch",
        json!({"queries":[{"path":".","matchString":"x","contextLines":2}]}),
    ));
    assert_eq!(both["matchString"], json!(["x"]));
    assert_eq!(both["contextLines"], json!({"before":2,"after":2}));
    let quoted = row(&accepted(
        "localSearch",
        json!({"queries":[{"path":".","matchString":"[\"x\",\"y\"]"}]}),
    ));
    assert_eq!(quoted["matchString"], json!(["x", "y"]));
    let number = row(&accepted("localFetch", json!({"queries":[{"path":5}]})));
    assert_eq!(number["path"], json!("5"));
    let single = row(&accepted(
        "localFetch",
        json!({"queries":[{"path":["a.ts"]}]}),
    ));
    assert_eq!(single["path"], json!("a.ts"));
}

#[test]
fn a_shape_with_one_reading_runs() {
    let bare = accepted("localSearch", json!({"path":".","matchString":["x"]}));
    assert_eq!(bare["queries"][0]["path"], json!("."));
    let with_budget = accepted(
        "localSearch",
        json!({"path":".","matchString":["x"],"responseChars":900}),
    );
    assert_eq!(with_budget["responseChars"], json!(900));
    assert!(with_budget["queries"][0].get("responseChars").is_none());
    let list = accepted("localFetch", json!([{"path":"a.ts"}]));
    assert_eq!(list["queries"][0]["path"], json!("a.ts"));
    let single = accepted("localFetch", json!({"queries":{"path":"a.ts"}}));
    assert_eq!(single["queries"][0]["path"], json!("a.ts"));
    let quoted = accepted("localFetch", json!({"queries":"[{\"path\":\"a.ts\"}]"}));
    assert_eq!(quoted["queries"][0]["path"], json!("a.ts"));
    let lifted = accepted(
        "localFetch",
        json!({"queries":[{"path":"a.ts","responseChars":100},{"path":"b.ts","responseChars":100}]}),
    );
    assert_eq!(lifted["responseChars"], json!(100));
    let spread = accepted(
        "localSearch",
        json!({"view":"files","queries":[{"path":"a","matchString":["x"]},{"path":"b","matchString":["y"],"view":"content"}]}),
    );
    assert_eq!(spread["queries"][0]["view"], json!("files"));
    assert_eq!(spread["queries"][1]["view"], json!("content"));
}

/// A list or scalar in place of a row is reported as what was sent.
#[test]
fn a_non_object_row_is_reported_as_sent() {
    for sent in [json!({"queries":[["a","b"]]}), json!({"queries":["a"]})] {
        let input = normalized_input("localSearch", sent.clone()).0;
        assert_eq!(input, sent);
    }
}

/// Fields only one operation takes name it; fields several take do not.
#[test]
fn fields_only_one_operation_takes_name_it() {
    let tree = accepted(
        "ghStructure",
        json!({"queries":[{"owner":"o","repo":"r","path":"src","maxDepth":1}]}),
    );
    assert_eq!(tree["queries"][0]["operation"], json!("tree"));
    let search = accepted(
        "ghSearchRepo",
        json!({"queries":[{"keywords":["x"],"limit":3}]}),
    );
    assert_eq!(search["queries"][0]["operation"], json!("search"));
    let many = normalized_input(
        "lspSearch",
        json!({"queries":[{"path":"a.ts","symbolName":"a","line":1}]}),
    )
    .0;
    assert!(many["queries"][0].get("operation").is_none());
}

#[test]
fn a_sort_takes_its_natural_direction() {
    let row = |sort: Value| {
        accepted(
            "structureSearch",
            json!({"queries":[{"path":".","operation":"files","sort":sort}]}),
        )["queries"][0]["sort"]
            .clone()
    };
    assert_eq!(row(json!("path")), json!({"by":"path","order":"asc"}));
    assert_eq!(
        row(json!({"by":"sizeBytes"})),
        json!({"by":"sizeBytes","order":"desc"})
    );
    assert_eq!(
        row(json!({"by":"sizeBytes","order":"asc"})),
        json!({"by":"sizeBytes","order":"asc"}),
        "a stated direction stays"
    );
}

/// More than one reading, or a guess: the agent decides, as the error asks.
#[test]
fn what_has_no_single_reading_stays_rejected() {
    for (tool, sent) in [
        ("localFetch", json!({"queries":[{"path":["a.ts","b.ts"]}]})),
        (
            "localFetch",
            json!({"queries":[{"path":"a.ts","ranges":"1-2"}]}),
        ),
        ("localFetch", json!({"query":{"path":"README.md"}})),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchString":["x"],"view":"file"}]}),
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a","responseChars":1},{"path":"b","responseChars":2}]}),
        ),
    ] {
        let input = normalized_input(tool, sent.clone()).0;
        let failed = prepare_rows(tool, input).map_or(true, |rows| rows.iter().any(Result::is_err));
        assert!(failed, "{sent} must stay rejected");
    }
}

/// An error names only what repair could not settle, never a field it repaired.
#[test]
fn an_error_names_only_what_repair_left() {
    let sent = json!({"queries":[
        {"path":".","matchString":"x","serchText":"typo"},
        {"path":".","matchString":"y"}
    ]});
    let input = normalized_input("localSearch", sent).0;
    assert_eq!(input["queries"][0]["serchText"], json!("typo"));
    assert_eq!(input["queries"][0]["matchString"], json!(["x"]));
    assert!(input["queries"][0].get("include").is_none());
    assert_eq!(input["queries"][1]["matchString"], json!(["y"]));
    let rows = prepare_rows("localSearch", input).expect("rows");
    let error = rows[0].as_ref().expect_err("the typo stays");
    let shown = format!("{:?}", error.issues);
    assert!(shown.contains("serchText"), "{shown}");
    assert!(!shown.contains("Expected a JSON array"), "{shown}");
}

#[test]
fn a_valid_call_is_untouched() {
    let sent = json!({"queries":[{"path":".","matchString":["x"]}],"responseChars":500});
    assert_eq!(normalized_input("localSearch", sent.clone()).0, sent);
}

/// A page size beside a cursor has one reading: the cursor keeps the size it
/// was issued with. The field is dropped, whatever its value, and the call
/// says so; any other field beside a cursor has no reading and stays for
/// validation to refuse.
#[test]
fn a_page_size_beside_a_cursor_is_dropped_and_said() {
    for size in [json!(60_000), json!("60000"), json!(null)] {
        let sent = json!({"cursor":"issued","responseChars":size});
        let (input, notes) = normalized_input("localSearch", sent.clone());
        assert_eq!(input, json!({"cursor":"issued"}), "{sent}");
        assert_eq!(notes.len(), 1, "{sent}: {notes:?}");
        assert!(
            notes[0].contains("`responseChars`")
                && notes[0].contains("page size it was issued with"),
            "{notes:?}"
        );
        crate::contracts::validate("localSearch", input).expect("the cursor alone is valid");
    }
    for sent in [
        json!({"cursor":"issued","path":"."}),
        json!({"cursor":"issued","responseChars":900,"path":"."}),
        json!({"cursor":"issued","queries":[{"path":".","matchString":["x"]}],"responseChars":900}),
    ] {
        let (input, notes) = normalized_input("localSearch", sent.clone());
        assert!(notes.is_empty(), "{sent}: {notes:?}");
        assert_eq!(input, sent);
        crate::contracts::validate("localSearch", input).expect_err("refused");
    }
}
