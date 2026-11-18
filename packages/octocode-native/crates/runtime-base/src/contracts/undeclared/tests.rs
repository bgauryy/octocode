use super::undeclared_paths;
use octocode_test_support::json_fixture;

#[test]
fn flags_only_keys_no_branch_declares() {
    let schema = json_fixture(
        r##"{
            "$defs": {
                "Row": {
                    "type": "object",
                    "properties": {"a": {"type": "string"}},
                    "additionalProperties": {}
                }
            },
            "type": "object",
            "properties": {
                "rows": {
                    "type": "array",
                    "items": {
                        "anyOf": [
                            {"$ref": "#/$defs/Row"},
                            {
                                "type": "object",
                                "properties": {"b": {"type": "object", "properties": {"c": {}}}}
                            }
                        ]
                    }
                },
                "free": {"type": "object", "additionalProperties": {}},
                "next": {"type": "object", "properties": {}}
            },
            "additionalProperties": {}
        }"##,
    );
    let output = json_fixture(
        r#"{
            "rows": [{"a": "x", "z": 1}, {"b": {"c": 1, "d": 2}}],
            "free": {"anything": 1},
            "next": {"page": {"tool": "t"}},
            "extra": true
        }"#,
    );
    assert_eq!(
        undeclared_paths(&schema, &output),
        vec![".extra", ".rows[].b.d", ".rows[].z"]
    );
}
