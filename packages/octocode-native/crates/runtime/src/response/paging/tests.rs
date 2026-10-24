use super::{CURSOR_MAXIMUM, mark_partial, next_cursor, push_warnings, terminal_limit};

/// A static notice is written once; a later page is told by its
/// cursors, never by the tool.
#[test]
fn notices_append_once_and_continuations_are_told_by_their_cursors() {
    let mut row = json!({});
    super::push_notice(&mut row, "static");
    super::push_notice(&mut row, "static");
    assert_eq!(row[super::NOTICES], json!(["static"]));
    for first in [json!({"path":"a"}), json!({"path":"a","page":1})] {
        assert!(
            !super::continues_a_page(ToolId::LspSearch, &first),
            "{first}"
        );
    }
    for later in [
        json!({"page":2}),
        json!({"snapshot":"s"}),
        json!({"importerPage":2}),
    ] {
        assert!(
            super::continues_a_page(ToolId::LspSearch, &later),
            "{later}"
        );
    }
}

/// A page past the last writes the shared outcome once: the flagged
/// block where the tool pages (a row block or a section) and `reason`
/// "page N of M"; a page in range, or page 1 of nothing, writes nothing.
use crate::id::ToolId;
use serde_json::{Map, Value, json};

/// One next-page builder: the query with the cursor advanced and the
/// pins written, as an exact-replay call; past the contract maximum the
/// row ends at a terminal limit and offers no call.
#[test]
fn next_page_call_advances_one_cursor_or_ends_at_the_terminal_limit() {
    let query = json!({"operation": "references", "path": "a.rs", "symbolName": "f",
            "line": 1, "page": 2});
    let mut row = json!({"matches": []});
    let call = super::next_page_call(
        &mut row,
        ToolId::LspSearch,
        None,
        query.clone(),
        "page",
        3,
        &[("snapshot", json!("lsp-v3:x"))],
    )
    .expect("an admitted cursor");
    assert_eq!(call["tool"], "lspSearch", "{call}");
    let next = &call["input"]["queries"][0];
    assert_eq!(next["page"], 3, "{call}");
    assert_eq!(next["snapshot"], "lsp-v3:x", "{call}");
    assert_eq!(next["path"], "a.rs", "{call}");
    assert_eq!(
        row,
        json!({"matches": []}),
        "an admitted cursor marks nothing"
    );
    let maximum = crate::contracts::query_schema_max(ToolId::LspSearch, None, "page");
    let past = super::next_page_call(
        &mut row,
        ToolId::LspSearch,
        None,
        query,
        "page",
        maximum + 1,
        &[],
    );
    assert!(past.is_none());
    assert_eq!(row["terminalLimit"], true, "{row}");
    assert_eq!(row["partialReasons"], json!([CURSOR_MAXIMUM]), "{row}");
}

/// The partial owners write a row or its object alike: a reason is
/// appended after the row's own once, a warning is appended once.
#[test]
fn partial_marks_append_once_on_a_row_or_its_object() {
    let mut object: Map<String, Value> = json!({
        "isPartial": true,
        "partialReasons": ["scopeCut"],
        "warnings": ["first"]
    })
    .as_object()
    .cloned()
    .expect("object");
    mark_partial(&mut object, "pageCeiling", &["ceiling".to_owned()]);
    mark_partial(&mut object, "pageCeiling", &["ceiling".to_owned()]);
    push_warnings(&mut object, ["first".to_owned(), "second".to_owned()]);
    assert_eq!(
        Value::Object(object.clone()),
        json!({"isPartial": true, "partialReasons": ["scopeCut", "pageCeiling"],
                "warnings": ["first", "ceiling", "second"]})
    );
    terminal_limit(&mut object, "pageCeiling", &[]);
    assert_eq!(object["terminalLimit"], true);
    assert_eq!(object["partialReasons"], json!(["scopeCut", "pageCeiling"]));
    let mut row = json!({"partialReasons": "malformed"});
    mark_partial(&mut row, "cap", &[]);
    assert_eq!(row, json!({"partialReasons": ["cap"], "isPartial": true}));
    let mut scalar = json!(1);
    push_warnings(&mut scalar, ["x".to_owned()]);
    assert_eq!(scalar, json!(1), "a non-object row is left alone");
}

/// Every bounded numeric cursor field of every tool's query schema, per
/// variant: `(tool, operation, field, maximum)`. Cursor fields are the
/// one list ([`crate::response::result::CURSOR_FIELDS`]) less a content
/// `offset`, which advances by its window, not by one.
fn bounded_cursors() -> Vec<(ToolId, Option<String>, String, u64)> {
    let mut found = Vec::new();
    for tool in ToolId::ALL {
        let contract = crate::contracts::tool_contract(tool).expect("tool contract");
        let schema = &contract["querySchema"];
        let defs = &schema["$defs"];
        let branches = ["oneOf", "anyOf"]
            .iter()
            .find_map(|key| schema.get(*key).and_then(Value::as_array))
            .map_or_else(|| vec![schema], |branches| branches.iter().collect());
        for branch in branches {
            let branch = crate::contracts::resolve_ref(branch, defs);
            let fields = branch["properties"].as_object().into_iter().flatten();
            let operation = branch["properties"]["operation"]["const"]
                .as_str()
                .map(str::to_owned);
            for (field, field_schema) in fields {
                let field_schema = crate::contracts::resolve_ref(field_schema, defs);
                let cursor =
                    field != "offset" && crate::contracts::state_fields(tool).contains(field);
                if let Some(maximum) = field_schema["maximum"].as_u64().filter(|_| cursor) {
                    found.push((tool, operation.clone(), field.clone(), maximum));
                }
            }
        }
    }
    found
}

/// Each bounded cursor: its maximum is the last value a continuation
/// names; one past it is a terminal limit naming the field, never a lead.
#[test]
fn every_bounded_cursor_ends_at_its_contract_maximum() {
    let cursors = bounded_cursors();
    for (tool, field) in [
        (ToolId::LspSearch, "page"),
        (ToolId::LspSearch, "importerPage"),
        (ToolId::GhStructure, "materializeOffset"),
        (ToolId::StructureSearch, "scanOffset"),
        (ToolId::LocalSearch, "matchPage"),
    ] {
        assert!(
            cursors.iter().any(|(t, _, f, _)| *t == tool && f == field),
            "{tool:?} {field} is found: {cursors:?}"
        );
    }
    for (tool, operation, field, maximum) in cursors {
        let maximum = usize::try_from(maximum).expect("usize maximum");
        let mut row = json!({});
        assert_eq!(
            next_cursor(&mut row, tool, operation.as_deref(), &field, maximum),
            Some(maximum),
            "{tool:?} {operation:?} {field}"
        );
        assert_eq!(row, json!({}), "an admitted cursor marks nothing");
        if maximum == usize::MAX {
            continue;
        }
        assert_eq!(
            next_cursor(&mut row, tool, operation.as_deref(), &field, maximum + 1),
            None,
            "{tool:?} {operation:?} {field}"
        );
        assert_eq!(row["terminalLimit"], true, "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        assert_eq!(row["partialReasons"], json!([CURSOR_MAXIMUM]), "{row}");
        let warning = row["warnings"][0].as_str().expect("warning");
        assert!(warning.contains(&format!("{field}:{maximum}")), "{warning}");
    }
}
