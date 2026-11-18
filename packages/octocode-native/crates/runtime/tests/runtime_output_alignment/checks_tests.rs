use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

fn envelope(data: Value) -> Value {
    json!({"type":"object","properties":{"results":{"type":"array","items":{
        "type":"object","properties":{"data":data}
    }}}})
}

fn findings(data: &Value, schema: &Value, defs: &Value) -> Findings {
    let mut found = Findings::new();
    uniform_with_schema(
        &mut Report {
            tool: "synthetic",
            findings: &mut found,
        },
        data,
        schema,
        defs,
    );
    found
}

#[test]
fn opaque_records_and_their_descendants_remain_source_local() {
    let schema = envelope(json_fixture(
        r#"{
            "type": "object",
            "properties": {
                "rows": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "kind": {"type": "string"},
                            "location": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {"path": {"type": "string"}, "line": {"type": "integer"}}
                            },
                            "opaque": {"type": "object", "additionalProperties": {}}
                        }
                    }
                }
            }
        }"#,
    ));
    let row = json_fixture(
        r#"{
            "kind": "function",
            "location": {"path": "a.rs", "line": 7},
            "opaque": {
                "source": "provider",
                "nested": [{"kind": "x"}, {"kind": "x"}, {"kind": "x"}]
            }
        }"#,
    );
    let data = json!({"rows":vec![row;UNIFORM_ROWS]});
    let before = data.clone();
    assert_eq!(
        findings(&data, &schema, &json!({})),
        BTreeSet::from([
            "B5 stated once | synthetic | rows[].kind".to_owned(),
            "B5 stated once | synthetic | rows[].location".to_owned(),
        ])
    );
    assert_eq!(data, before, "the checker never changes source evidence");
}

#[test]
fn renamed_opaque_boundaries_follow_references_and_unions() {
    let defs = json_fixture(
        r##"{
            "$defs": {
                "opaque/~": {"type": "object", "additionalProperties": true},
                "entry": {
                    "allOf": [
                        {
                            "type": "object",
                            "properties": {"label": {"type": "string"}}
                        },
                        {
                            "type": "object",
                            "properties": {
                                "payload": {
                                    "anyOf": [{"type": "null"}, {"$ref": "#/$defs/opaque~1~0"}]
                                }
                            }
                        }
                    ]
                },
                "data": {
                    "type": "object",
                    "properties": {
                        "batch": {"type": "array", "items": {"$ref": "#/$defs/entry"}}
                    }
                }
            }
        }"##,
    );
    let schema = envelope(json_fixture(
        r##"{"anyOf": [{"type": "null"}, {"$ref": "#/$defs/data"}]}"##,
    ));
    let row = json_fixture(
        r#"{
            "label": "shared",
            "payload": {"arbitrary": [{"signal": 1}, {"signal": 1}, {"signal": 1}]}
        }"#,
    );
    let data = json!({"batch":vec![row;UNIFORM_ROWS]});
    assert_eq!(
        findings(&data, &schema, &defs),
        BTreeSet::from(["B5 stated once | synthetic | batch[].label".to_owned()])
    );
}

#[test]
fn published_scalar_and_structured_candidates_still_flag() {
    let schema = envelope(json_fixture(
        r#"{
            "type": "object",
            "properties": {
                "entries": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "mode": {"type": "boolean"},
                            "anchor": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {"offset": {"type": "integer"}}
                            }
                        }
                    }
                }
            }
        }"#,
    ));
    let row = json_fixture(r#"{"mode": true, "anchor": {"offset": 12}}"#);
    let data = json!({"entries":vec![row;UNIFORM_ROWS]});
    assert_eq!(
        findings(&data, &schema, &json!({})),
        BTreeSet::from([
            "B5 stated once | synthetic | entries[].anchor".to_owned(),
            "B5 stated once | synthetic | entries[].mode".to_owned(),
        ])
    );
}

#[test]
fn a_present_unresolved_reference_is_a_contract_diagnostic() {
    let schema = envelope(json_fixture(
        r##"{
            "type": "object",
            "properties": {
                "rows": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["kind"],
                        "properties": {"kind": {"$ref": "#/$defs/missing"}}
                    }
                }
            }
        }"##,
    ));
    let data = json!({"rows":vec![json_fixture(r#"{"kind": "shared"}"#);UNIFORM_ROWS]});
    assert_eq!(
        findings(&data, &schema, &json!({})),
        BTreeSet::from([
            "B5 contract route | synthetic | unresolved reference #/$defs/missing at rows[].kind"
                .to_owned()
        ])
    );
}

#[test]
fn an_absent_optional_reference_is_not_traversed() {
    let schema = envelope(json_fixture(
        r##"{
            "type": "object",
            "properties": {"optional": {"$ref": "#/$defs/missing"}}
        }"##,
    ));
    assert!(findings(&json!({}), &schema, &json!({})).is_empty());
}

#[test]
fn reference_cycles_terminate_and_real_recursive_values_remain_visible() {
    let defs = json_fixture(
        r##"{
            "$defs": {
                "node": {
                    "type": "object",
                    "allOf": [{"$ref": "#/$defs/loop"}],
                    "properties": {
                        "kind": {"type": "string"},
                        "metadata": {"type": "object", "additionalProperties": {}},
                        "children": {"type": "array", "items": {"$ref": "#/$defs/node"}}
                    }
                },
                "loop": {"$ref": "#/$defs/node"}
            }
        }"##,
    );
    let schema = envelope(json_fixture(
        r##"{
            "type": "object",
            "properties": {
                "nodes": {"type": "array", "items": {"$ref": "#/$defs/node"}}
            }
        }"##,
    ));
    let leaf = json_fixture(r#"{"kind": "child", "metadata": {"source": "provider"}}"#);
    let node = json!({"kind":"parent","metadata":{"source":"provider"},
        "children":vec![leaf;UNIFORM_ROWS]});
    let data = json!({"nodes":vec![node;UNIFORM_ROWS]});
    assert_eq!(
        findings(&data, &schema, &defs),
        BTreeSet::from([
            "B5 stated once | synthetic | nodes[].kind".to_owned(),
            "B5 stated once | synthetic | nodes[].children[].kind".to_owned(),
        ])
    );
}
