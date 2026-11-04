use super::*;
use crate::contracts::prepare_rows;
use serde_json::json;

fn accepted(tool: &str, sent: Value) -> Value {
    let input = normalized_input(tool, sent.clone());
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

/// One date and person shape for every operation: a bare value means `include`.
#[test]
fn a_filter_selection_takes_its_value_bare() {
    let range = json!({"from":"2024-01-01T00:00:00Z","to":"2024-02-01T00:00:00Z"});
    let row = |operation: &str, filters: Value| json!({"queries":[{"operation":operation,"owner":"o","repo":"r","filters":filters}]});
    let pull = accepted(
        "ghSearchHistory",
        row("pullRequest", json!({"createdAt":range,"author":"octocat"})),
    );
    let filters = &pull["queries"][0]["filters"];
    assert_eq!(filters["createdAt"], json!({"include":[range]}));
    assert_eq!(filters["author"], json!({"include":["octocat"]}));
    let commits = accepted(
        "ghSearchHistory",
        row("commit", json!({"committedAt":range})),
    );
    assert_eq!(commits["queries"][0]["filters"]["committedAt"], range);
}

/// Several values of a selection are its `include`, as one value is; a filter
/// that takes fewer says so of the values sent.
#[test]
fn a_filter_selection_takes_a_bare_list_of_values() {
    let row = |filters: Value| json!({"queries":[{"operation":"pullRequest","owner":"o","repo":"r","filters":filters}]});
    let pull = accepted("ghSearchHistory", row(json!({"labels":["bug","ui"]})));
    assert_eq!(
        pull["queries"][0]["filters"]["labels"],
        json!({"include":["bug","ui"]})
    );
    let one = accepted("ghSearchHistory", row(json!({"labels":["bug"]})));
    assert_eq!(
        one["queries"][0]["filters"]["labels"],
        json!({"include":["bug"]})
    );

    let input = normalized_input("ghSearchHistory", row(json!({"assignee":["a","b"]})));
    let error = prepare_rows("ghSearchHistory", input).expect_err("two assignees");
    let shown = format!("{:?}", error.issues);
    assert!(shown.contains("Value length 2 exceeds the maximum of 1"), "{shown}");
    assert!(!shown.contains("\"include\", \"0\""), "{shown}");
}

/// A row is no selection though it declares `include`: a list or scalar in its
/// place is reported as what was sent, not as an `include` nobody wrote.
#[test]
fn a_row_with_an_include_field_is_no_selection() {
    for sent in [json!({"queries":[["a","b"]]}), json!({"queries":["a"]})] {
        let input = normalized_input("localSearch", sent.clone());
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
    );
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

#[test]
fn a_cursor_keeps_the_page_size_it_was_issued_with() {
    let input = normalized_input("localSearch", json!({"cursor":"c","responseChars":900}));
    assert_eq!(input, json!({"cursor":"c"}));
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
            "localSearch",
            json!({"queries":[{"path":".","pattern":"x"}]}),
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a","responseChars":1},{"path":"b","responseChars":2}]}),
        ),
    ] {
        let input = normalized_input(tool, sent.clone());
        let failed = prepare_rows(tool, input).map_or(true, |rows| rows.iter().any(Result::is_err));
        assert!(failed, "{sent} must stay rejected");
    }
}

/// A row whose other fields happen to include `include` is not a selection, and
/// an error names only what repair could not settle, never a field it repaired.
#[test]
fn an_error_names_only_what_repair_left() {
    let sent = json!({"queries":[
        {"path":".","matchString":"x","serchText":"typo"},
        {"path":".","matchString":"y"}
    ]});
    let input = normalized_input("localSearch", sent);
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
    assert_eq!(normalized_input("localSearch", sent.clone()), sent);
}

/// A commit list takes one date range and a commit search a `{include, exclude}`
/// selection: a one-item selection on the list means that range, and the error
/// must not send the caller after `keywords` it does not want.
#[test]
fn a_one_item_date_selection_reads_as_the_range_the_form_takes() {
    let range = json!({"from":"2025-01-01T00:00:00Z","to":"2025-02-01T00:00:00Z"});
    let row = |filters: Value, keywords: Option<Value>| {
        let mut row = json!({"operation":"commit","owner":"a","repo":"b","filters":filters});
        if let Some(keywords) = keywords {
            row["keywords"] = keywords;
        }
        json!({"queries":[row]})
    };
    let listed = accepted(
        "ghSearchHistory",
        row(json!({"committedAt":{"include":[range.clone()]}}), None),
    );
    assert_eq!(listed["queries"][0]["filters"]["committedAt"], range);
    let searched = accepted(
        "ghSearchHistory",
        row(
            json!({"committedAt":{"include":[range.clone()]}}),
            Some(json!(["fix"])),
        ),
    );
    assert_eq!(
        searched["queries"][0]["filters"]["committedAt"]["include"][0], range,
        "the search form keeps its selection"
    );
    // An exclusion has no reading on the list: it stays as sent, and rejected.
    let excluded = row(json!({"committedAt":{"exclude":[range.clone()]}}), None);
    assert_eq!(
        normalized_input("ghSearchHistory", excluded.clone()),
        excluded
    );
}
