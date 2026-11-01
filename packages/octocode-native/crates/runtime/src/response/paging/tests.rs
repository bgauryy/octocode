use super::{mark_partial, next_page_call, push_warnings, terminal_limit};

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
/// pins written, as an exact-replay call, however far past any former
/// schema ceiling the cursor runs.
#[test]
fn next_page_call_advances_one_cursor_past_any_size() {
    let query = json!({"operation": "references", "path": "a.rs", "symbolName": "f",
            "line": 1, "page": 2});
    for next in [3, 1_001, 100_001, usize::MAX] {
        let call = next_page_call(
            ToolId::LspSearch,
            query.clone(),
            "page",
            next,
            &[("snapshot", json!("lsp-v3:x"))],
        )
        .expect("an object query");
        assert_eq!(call["tool"], "lspSearch", "{call}");
        let row = &call["input"]["queries"][0];
        assert_eq!(row["page"], json!(next), "{call}");
        assert_eq!(row["snapshot"], "lsp-v3:x", "{call}");
        assert_eq!(row["path"], "a.rs", "{call}");
    }
    assert!(next_page_call(ToolId::LspSearch, json!(1), "page", 2, &[]).is_none());
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
    mark_partial(
        &mut object,
        crate::response::paging::PartialReason::WalkWindow,
        &["ceiling".to_owned()],
    );
    mark_partial(
        &mut object,
        crate::response::paging::PartialReason::WalkWindow,
        &["ceiling".to_owned()],
    );
    push_warnings(&mut object, ["first".to_owned(), "second".to_owned()]);
    assert_eq!(
        Value::Object(object.clone()),
        json!({"isPartial": true, "partialReasons": ["scopeCut", "walkWindow"],
                "warnings": ["first", "ceiling", "second"]})
    );
    terminal_limit(
        &mut object,
        crate::response::paging::PartialReason::WalkWindow,
        &[],
    );
    assert_eq!(object["terminalLimit"], true);
    assert_eq!(object["partialReasons"], json!(["scopeCut", "walkWindow"]));
    let mut row = json!({"partialReasons": "malformed"});
    mark_partial(
        &mut row,
        crate::response::paging::PartialReason::ProviderResultCap,
        &[],
    );
    assert_eq!(
        row,
        json!({"partialReasons": ["providerResultCap"], "isPartial": true})
    );
    let mut scalar = json!(1);
    push_warnings(&mut scalar, ["x".to_owned()]);
    assert_eq!(scalar, json!(1), "a non-object row is left alone");
}

/// No continuation position of any tool has a schema ceiling: a cursor is
/// a private position, so the native side never needs one to stop at. The
/// one bounded private field is ghGetHistoryItem's `pageSize`, a provider
/// batch size rather than a position.
#[test]
fn no_cursor_field_declares_a_maximum() {
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
            for (field, field_schema) in branch["properties"].as_object().into_iter().flatten() {
                let field_schema = crate::contracts::resolve_ref(field_schema, defs);
                let batch_size = tool == ToolId::GhGetHistoryItem && field == "pageSize";
                if field_schema["x-cursor"] == json!(true) && !batch_size {
                    assert!(
                        field_schema.get("maximum").is_none(),
                        "{tool:?} {field}: {field_schema}"
                    );
                }
            }
        }
    }
}
