use super::{
    CURSOR_FIELDS, ToolError, for_each_call, for_each_call_mut, restart_query, stale_snapshot,
    with_changes,
};
use crate::id::ToolId;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

/// Every query field of every tool that names where a walk stands (a
/// page, an offset, a snapshot, a saved selection) is in the one list a
/// restart drops, and every listed field is some tool's query field.
#[test]
fn the_cursor_list_is_every_contract_cursor_field() {
    let mut fields = std::collections::BTreeSet::new();
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
            for field in branch["properties"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(f, _)| f)
            {
                fields.insert(field.clone());
            }
        }
    }
    let cursor = |field: &str| {
        matches!(
            field,
            "page" | "offset" | "snapshot" | "selection" | "cursor"
        ) || ["Page", "Offset", "Snapshot"]
            .iter()
            .any(|suffix| field.ends_with(suffix))
    };
    let found: Vec<&String> = fields.iter().filter(|field| cursor(field)).collect();
    let listed: Vec<&str> = CURSOR_FIELDS.to_vec();
    for field in &found {
        assert!(
            listed.contains(&field.as_str()),
            "{field} is a cursor field"
        );
    }
    for field in &listed {
        assert!(fields.contains(*field), "{field} is no query field");
    }
}

/// A restart is page 1 of the same query: every cursor and the saved
/// selection dropped, a replay, executable as built.
#[test]
fn a_restart_drops_every_cursor_and_the_saved_selection() {
    let row = json!({"path":"src","matchString":"f","page":3,"matchPage":2,
            "snapshot":"s1","selection":"plan","pageSize":5});
    let restart = restart_query(ToolId::LocalSearch, &row);
    assert_eq!(restart["tool"], "localSearch");
    assert!(restart.get("confidence").is_none());
    assert_eq!(
        restart["query"]["queries"][0],
        json!({"path":"src","matchString":"f","pageSize":5})
    );
}

/// The stale row a tool returns, as a row or as a whole-call error,
/// carries the code and the restart; the runtime owns its text.
#[test]
fn a_stale_snapshot_carries_its_restart() {
    let restart = restart_query(
        ToolId::LocalFetch,
        &json!({"path":"a","offset":90,"snapshot":"s"}),
    );
    let row = stale_snapshot(restart.clone());
    assert_eq!(row["errorCode"], "staleSnapshot");
    assert_eq!(row["next"]["restart"], restart);
    let error = ToolError::stale_snapshot(restart.clone());
    assert_eq!(error.code, ErrorCode::StaleSnapshot);
    assert_eq!(error.next.as_deref(), Some(&json!({"restart": restart})));
}

/// A change sets its field in place, a `null` change drops it, and the
/// fields left keep their order.
#[test]
fn changes_set_fields_and_nulls_drop_them() {
    let row = json!({"path":"a","page":2,"pageSize":5,"snapshot":"s"});
    let changed = with_changes(row, &json!({"page":3,"snapshot":null,"depth":1}));
    assert_eq!(
        serde_json::to_string(&changed).expect("json"),
        r#"{"path":"a","page":3,"pageSize":5,"depth":1}"#
    );
}

/// The call walk visits every `{tool, query}` at any depth (under `next`,
/// `hints`, items, a response page) and never enters a call's query.
#[test]
fn the_call_walk_visits_every_call_and_never_its_query() {
    let inner = json!({"tool":"localFetch","query":{"queries":[{"path":"b"}]}});
    let mut value = json!({
        "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":"a"}],
            "nested":inner}}},
        "items":[{"hints":[inner]}],
        "tool":"not a call"
    });
    let mut tools = Vec::new();
    for_each_call(&value, &mut |call| tools.push(call["tool"].clone()));
    assert_eq!(tools, vec![json!("localSearch"), json!("localFetch")]);
    for_each_call_mut(&mut value, &mut |call| call["visited"] = json!(true));
    assert_eq!(value["next"]["nextPage"]["visited"], true);
    assert!(
        value["next"]["nextPage"]["query"]["nested"]
            .get("visited")
            .is_none()
    );
    assert_eq!(value["items"][0]["hints"][0]["visited"], true);
}
