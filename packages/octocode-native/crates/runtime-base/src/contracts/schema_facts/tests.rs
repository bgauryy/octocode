use super::*;
use octocode_test_support::json_fixture;

#[test]
fn property_navigation_preserves_schemas_and_stops_reference_cycles() {
    let root = json_fixture(
        r##"{
            "$defs": {
                "success": {
                    "allOf": [
                        {
                            "properties": {
                                "results": {"type": "array", "items": {"$ref": "#/$defs/row"}}
                            }
                        },
                        {"$ref": "#/$defs/success"}
                    ]
                },
                "row": {
                    "oneOf": [
                        {
                            "properties": {
                                "data": {
                                    "allOf": [
                                        {"required": ["base"]},
                                        {"oneOf": [{"required": ["left"]}, {"required": ["right"]}]}
                                    ]
                                }
                            }
                        },
                        {"properties": {"data": {"required": ["error"]}}}
                    ]
                }
            },
            "anyOf": [{"required": ["error"]}, {"$ref": "#/$defs/success"}]
        }"##,
    );
    let defs = &root["$defs"];
    let results = property_schemas(&root, defs, "results");
    assert_eq!(results.len(), 1);
    let data = property_schemas(&results[0]["items"], defs, "data");
    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["allOf"][0]["required"], json_fixture(r#"["base"]"#));
    assert_eq!(
        data[0]["allOf"][1]["oneOf"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(data[1]["required"], json_fixture(r#"["error"]"#));
    assert!(property_schemas(&root, defs, "missing").is_empty());
}

/// The graphQuery page bounds are the contract's: a `default` declared
/// beside the `$ref` to the bounded page type is read, not lost.
#[test]
fn keywords_beside_a_ref_are_read_along_the_chain() {
    assert_eq!(
        query_schema_number(ToolId::GraphQuery, None, "limit", "default"),
        Some(50)
    );
    assert_eq!(
        query_schema_number(ToolId::GraphQuery, None, "limit", "minimum"),
        Some(1)
    );
}

/// A default is the value an omitted field parses to: a variant that
/// requires the field (a `view` const) cannot disagree with it, while two
/// optional variants declaring different defaults leave none.
#[test]
fn a_default_is_read_from_the_variants_where_the_field_is_optional() {
    let schema = json_fixture(
        r#"{
            "$defs": {},
            "oneOf": [
                {
                    "properties": {
                        "operation": {"const": "definition"},
                        "view": {"default": "content"}
                    }
                },
                {
                    "required": ["view"],
                    "properties": {
                        "operation": {"const": "definition"},
                        "view": {"const": "files"}
                    }
                },
                {
                    "properties": {"operation": {"const": "callers"}, "depth": {"default": 1}}
                },
                {
                    "properties": {"operation": {"const": "callers"}, "depth": {"default": 2}}
                },
                {
                    "required": ["line"],
                    "properties": {"operation": {"const": "hover"}, "line": {"minimum": 1}}
                }
            ]
        }"#,
    );
    let defs = &schema["$defs"];
    let variants = schema["oneOf"]
        .as_array()
        .expect("branches")
        .iter()
        .map(|branch| Variant::of(branch, defs))
        .collect::<Vec<_>>();
    let value =
        |operation, field, keyword| declared_value(&variants, Some(operation), field, keyword);
    assert_eq!(
        value("definition", "view", "default"),
        Some(&serde_json::json!("content"))
    );
    assert_eq!(
        value("callers", "depth", "default"),
        None,
        "optional variants disagree"
    );
    assert_eq!(
        value("hover", "line", "default"),
        None,
        "no optional variant remains"
    );
    assert_eq!(
        value("hover", "line", "minimum"),
        Some(&serde_json::json!(1)),
        "only defaults skip a requiring variant"
    );
    // The live lspSearch contract has this shape.
    assert_eq!(
        query_schema_value(ToolId::LspSearch, Some("definition"), "view", "default"),
        Some(&serde_json::json!("content"))
    );
}

/// A field core bounds by nothing reads as unbounded, never a panic.
#[test]
fn an_undeclared_bound_reads_as_none() {
    assert_eq!(query_schema_max(ToolId::GhSearchRepo, None, "nope"), None);
    assert_eq!(query_schema_max(ToolId::GraphQuery, None, "depth"), None);
    assert_eq!(query_schema_max(ToolId::GraphQuery, None, "limit"), None);
}
