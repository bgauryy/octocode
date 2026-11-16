use super::{mark_partial, next_candidate_page_call, push_warnings, terminal_limit};

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
    let mut checked = 0;
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
                let batch_size = tool == ToolId::GhGetHistoryItem && field == "pageSize";
                if crate::contracts::schema_keyword(field_schema, defs, "x-cursor")
                    == Some(&json!(true))
                    && !batch_size
                {
                    checked += 1;
                    assert!(
                        crate::contracts::schema_keyword(field_schema, defs, "maximum").is_none(),
                        "{tool:?} {field}: {field_schema}"
                    );
                }
            }
        }
    }
    assert!(
        checked > 0,
        "the cursor invariant must inspect declared positions"
    );
}

/// The candidate page of a replayed scan restarts its private progress and
/// carries the selection token and the scan's snapshot.
#[test]
fn next_candidate_page_call_restarts_progress_and_names_the_selection() {
    let query = json!({"path": "src", "matchString": ["x"], "matchPage": 3, "snapshot": "old"});
    let call = next_candidate_page_call(ToolId::LocalSearch, query, "token-1", Some("snap-2"))
        .expect("an object query");
    let row = &call["input"]["queries"][0];
    assert_eq!(row["selection"], "token-1", "{call}");
    assert_eq!(row["snapshot"], "snap-2", "{call}");
    assert!(row.get("matchPage").is_none(), "{call}");
    assert_eq!(row["path"], "src", "{call}");
}

/// A candidate page keeps its query's snapshot pin unless the caller
/// passes a newer one; only private progress is dropped.
#[test]
fn candidate_page_keeps_the_query_snapshot_and_drops_progress() {
    let query = json!({"path": "src", "matchString": ["x"], "matchPage": 3, "snapshot": "pinned"});
    let call = next_candidate_page_call(ToolId::LocalSearch, query, "token-1", None)
        .expect("an object query");
    let row = &call["input"]["queries"][0];
    assert_eq!(row["snapshot"], "pinned", "{call}");
    assert!(row.get("matchPage").is_none(), "{call}");
    let local_fetch = json!({"path": "a.rs", "snapshot": "pinned", "readProgress": {"unit": "lines", "offset": 3}});
    let page = crate::response::result::with_changes(
        local_fetch,
        &super::candidate_page_changes(ToolId::LocalFetch, "token-1", None),
    );
    assert_eq!(page["snapshot"], "pinned", "{page}");
    assert!(page.get("readProgress").is_none(), "{page}");
}

/// A partial reason reaches a caller as its code alone: the row's own note
/// stays, and no response says what any code means (R1).
#[test]
fn a_reason_reaches_a_caller_as_its_code_alone() {
    let mut row = json!({"files": [], "matchCount": 0});
    mark_partial(
        &mut row,
        crate::response::paging::PartialReason::BuildScriptsOff,
        &[],
    );
    mark_partial(
        &mut row,
        crate::response::paging::PartialReason::CfgGatedFiles,
        &[],
    );
    super::push_notice(
        &mut row,
        "rust-analyzer did not run this package's build scripts.",
    );
    let response = crate::tools::test_support::staged_response(ToolId::LspSearch, vec![row]);
    assert_eq!(
        response["results"][0]["data"]["partialReasons"],
        json!(["buildScriptsOff", "cfgGatedFiles"]),
        "{response}"
    );
    assert!(
        response
            .to_string()
            .contains("rust-analyzer did not run this package's build scripts."),
        "the row's own note stays: {response}"
    );
    crate::tools::test_support::checks::assert_no_reason_meaning(&response);
}
