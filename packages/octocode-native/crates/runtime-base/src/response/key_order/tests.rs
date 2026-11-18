use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

/// A pass over one synthetic `data` schema, leaked for the test's life.
fn pass_over(data: Value) -> Pass {
    static NO_DEFS: Value = Value::Null;
    let data: Schema = Box::leak(Box::new(data));
    Pass {
        output: Places::new(&NO_DEFS, vec![data]),
        queries: HashMap::new(),
    }
}

/// The roast's F10 rows: three rows of one graphQuery list, each built in
/// its own order. Every row reads in the first row's order afterwards, and
/// holds what it held.
#[test]
fn rows_of_one_list_share_one_key_order() {
    let lead = json_fixture(
        r#"{
            "kind": "related",
            "tool": "lspSearch",
            "input": {
                "queries": [
                    {
                        "operation": "references",
                        "path": "src/a.ts",
                        "symbolName": "a",
                        "line": 2
                    }
                ]
            }
        }"#,
    );
    let built = json!({"results":[{"index":0,"data":{"rows":[
        {"id":"a.ts#a","kind":"symbol","file":"a.ts","line":2,"verify":[lead],"match":"words",
         "exported":true,"endLine":4,"symbolKind":"function","name":"a"},
        {"id":"b.ts#b","kind":"symbol","symbolKind":"function","file":"b.ts","verify":[lead],
         "match":"words","exported":true,"line":3,"name":"b"},
        {"id":"c.ts#c","kind":"symbol","name":"c","symbolKind":"function","verify":[lead],
         "match":"words","line":3,"file":"c.ts"},
    ]}}]});
    let mut ordered = built.clone();
    order_rows(ToolId::GraphQuery, &mut ordered);
    assert_eq!(ordered, built, "keys move; nothing is added or dropped");
    let rows = ordered["results"][0]["data"]["rows"]
        .as_array()
        .expect("rows");
    let first = keys(&rows[0]);
    assert_eq!(
        first,
        keys(&built["results"][0]["data"]["rows"][0]),
        "the first row keeps the order it was built in"
    );
    for row in &rows[1..] {
        let shared: Vec<&str> = first
            .iter()
            .copied()
            .filter(|key| row.get(key).is_some())
            .collect();
        assert_eq!(keys(row), shared);
    }
}

/// A key only a later row states, first in its row, follows the keys already
/// stated, on every row after it; rows of two result rows of one response share the order.
#[test]
fn a_key_a_later_row_adds_follows_the_stated_ones() {
    let schema = json_fixture(
        r#"{
            "properties": {
                "rows": {
                    "items": {"properties": {"a": {}, "b": {}, "c": {}, "d": {}}}
                }
            }
        }"#,
    );
    let mut pass = pass_over(schema);
    let mut first = json_fixture(
        r#"{
            "rows": [
                {"b": 1, "a": 1},
                {"d": 1, "a": 1, "b": 1},
                {"c": 1, "d": 1, "a": 1}
            ]
        }"#,
    );
    let mut second = json_fixture(r#"{"rows": [{"d": 1, "c": 1, "b": 1, "a": 1}]}"#);
    pass.order(&mut first, ROOT, false);
    pass.order(&mut second, ROOT, false);
    assert_eq!(keys(&first["rows"][0]), ["b", "a"]);
    assert_eq!(keys(&first["rows"][1]), ["b", "a", "d"]);
    assert_eq!(keys(&first["rows"][2]), ["a", "d", "c"]);
    assert_eq!(keys(&second["rows"][0]), ["b", "a", "d", "c"]);
}

/// Objects inside a list element share an order by their place, a key the
/// schema leaves open included; a record's keys (typed by value, possibly
/// ranked) and a row's own `data` keys stay as built.
#[test]
fn only_shaped_objects_inside_a_list_are_ordered() {
    let schema = json_fixture(
        r#"{
            "properties": {
                "rows": {
                    "items": {
                        "properties": {
                            "id": {},
                            "range": {"properties": {"start": {}, "end": {}}},
                            "byKind": {"additionalProperties": {"type": "integer"}}
                        }
                    }
                },
                "counts": {"properties": {"x": {}, "y": {}}}
            }
        }"#,
    );
    let mut pass = pass_over(schema);
    let built = json_fixture(
        r#"{
            "counts": {"y": 1, "x": 2},
            "rows": [
                {
                    "id": "a",
                    "range": {"start": 1, "end": 2},
                    "byKind": {"symbol": 9, "file": 1}
                },
                {
                    "extra": true,
                    "range": {"end": 4, "start": 3},
                    "byKind": {"file": 7, "symbol": 2},
                    "id": "b"
                }
            ]
        }"#,
    );
    let mut ordered = built.clone();
    pass.order(&mut ordered, ROOT, false);
    assert_eq!(ordered, built);
    assert_eq!(keys(&ordered), ["counts", "rows"]);
    assert_eq!(keys(&ordered["counts"]), ["y", "x"], "not in a list");
    let second = &ordered["rows"][1];
    assert_eq!(keys(second), ["id", "range", "byKind", "extra"]);
    assert_eq!(keys(&second["range"]), ["start", "end"]);
    assert_eq!(keys(&second["byKind"]), ["file", "symbol"], "a record");
}

/// A call states each query in the order its target publishes the fields
/// (`schema <tool>`), at any depth of the query and wherever the row holds
/// the call; a cursor continuation has nothing to order.
#[test]
fn an_emitted_call_states_its_queries_in_the_published_order() {
    let read = json_fixture(
        r#"{
            "tool": "localFetch",
            "kind": "related",
            "input": {
                "queries": [
                    {
                        "ranges": [{"endLine": 29, "startLine": 29}],
                        "view": "content",
                        "path": "src/a.ts"
                    }
                ]
            }
        }"#,
    );
    let cursor = json_fixture(
        r#"{
            "kind": "continue",
            "tool": "localSearch",
            "input": {"cursor": "c"}
        }"#,
    );
    let built = json!({"results":[{"index":0,"data":{
        "files":[],
        "next":{"nextPage":cursor,"read":read},
        "related":[read],
    }}]});
    let mut ordered = built.clone();
    order_rows(ToolId::LocalSearch, &mut ordered);
    assert_eq!(ordered, built);
    let data = &ordered["results"][0]["data"];
    for call in [&data["next"]["read"], &data["related"][0]] {
        let query = &call["input"]["queries"][0];
        assert_eq!(keys(query), ["path", "view", "ranges"]);
        assert_eq!(keys(&query["ranges"][0]), ["startLine", "endLine"]);
    }
    assert_eq!(data["next"]["nextPage"], cursor);
}

/// The published order is core's declaration order, which the tool
/// contract's own schemas (keys by name) do not keep: the two differ for a
/// tool that declares `path` before `limit`.
#[test]
fn the_published_query_order_is_the_declared_one() {
    let mut places = query_places(ToolId::LocalFetch).expect("localFetch publishes its input");
    let declared = places.declared(ROOT).to_vec();
    let at = |name: &str| declared.iter().position(|key| *key == name);
    assert!(
        at("path").is_some() && at("path") < at("limit"),
        "{declared:?}"
    );
    for tool in ToolId::ALL {
        let mut places = query_places(tool).expect("every tool publishes its input");
        assert!(
            !places.declared(ROOT).is_empty(),
            "{tool:?} publishes no query field"
        );
    }
}

/// Ordering an ordered response changes nothing.
#[test]
fn ordering_twice_is_ordering_once() {
    let mut once = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "rows": [
                            {"kind": "file", "id": "a.ts"},
                            {"id": "b.ts", "language": "typescript", "kind": "file"}
                        ]
                    }
                }
            ]
        }"#,
    );
    order_rows(ToolId::GraphQuery, &mut once);
    let mut twice = once.clone();
    order_rows(ToolId::GraphQuery, &mut twice);
    let rows = |value: &Value| {
        value["results"][0]["data"]["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| keys(row).join(","))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&once), ["kind,id", "kind,id,language"]);
    assert_eq!(rows(&once), rows(&twice));
}

/// A key a later element adds keeps the place that element built it in: a
/// localSearch list whose first file has no `declarations` used to move every
/// later file's `declarations` after its `matches`, so a split row sent the
/// hits before the declarations they index.
#[test]
fn a_key_a_later_element_adds_keeps_its_built_place() {
    let mut rows = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "matchCount": 2,
                        "files": [
                            {
                                "path": "a.rs",
                                "matches": [{"line": 1, "value": "pub fn needle() {}"}]
                            },
                            {
                                "path": "b.rs",
                                "declarations": [
                                    {
                                        "symbolName": "f",
                                        "kind": "function",
                                        "line": 2,
                                        "endLine": 4
                                    }
                                ],
                                "matches": [{"line": 3, "value": "    needle();", "enclosing": 0}]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let before = rows.clone();
    order_rows(ToolId::LocalSearch, &mut rows);
    let files = &rows["results"][0]["data"]["files"];
    assert_eq!(keys(&files[0]), ["path", "matches"]);
    assert_eq!(keys(&files[1]), ["path", "declarations", "matches"]);
    assert_eq!(rows, before, "the pass moves keys only");
}

/// Leading additions follow the established keys, while additions with a
/// preceding anchor keep their place between the keys already known.
#[test]
fn leading_additions_append_and_anchored_additions_keep_their_place() {
    let schema = json_fixture(
        r#"{
            "properties": {
                "rows": {
                    "items": {
                        "properties": {
                            "left": {},
                            "right": {},
                            "leadOne": {},
                            "leadTwo": {},
                            "middleOne": {},
                            "middleTwo": {}
                        }
                    }
                }
            }
        }"#,
    );
    let mut pass = pass_over(schema);
    let built = json_fixture(
        r#"{
            "rows": [
                {"left": 1, "right": 2},
                {
                    "leadOne": 3,
                    "leadTwo": 4,
                    "left": 5,
                    "middleOne": 6,
                    "middleTwo": 7,
                    "right": 8
                },
                {
                    "right": 9,
                    "leadTwo": 10,
                    "middleTwo": 11,
                    "left": 12,
                    "leadOne": 13,
                    "middleOne": 14
                }
            ]
        }"#,
    );
    let mut ordered = built.clone();
    pass.order(&mut ordered, ROOT, false);
    assert_eq!(ordered, built, "the pass moves keys only");
    assert_eq!(keys(&ordered["rows"][0]), ["left", "right"]);
    for row in ordered["rows"].as_array().expect("rows").iter().skip(1) {
        assert_eq!(
            keys(row),
            [
                "left",
                "middleOne",
                "middleTwo",
                "right",
                "leadOne",
                "leadTwo"
            ]
        );
    }
}
