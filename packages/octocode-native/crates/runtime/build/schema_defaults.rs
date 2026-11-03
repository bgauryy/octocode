//! Build-time facts about executable schema positions, never instance data.
use serde_json::Value;

pub fn has_default(schema: &Value, root: &Value) -> bool {
    let mut pending = vec![(schema, schema)];
    let mut seen = Vec::new();
    while let Some((schema, scope)) = pending.pop() {
        let scope = if schema.get("$defs").is_some() {
            schema
        } else {
            scope
        };
        let identity = (std::ptr::from_ref(schema), std::ptr::from_ref(scope));
        if seen.contains(&identity) {
            continue;
        }
        seen.push(identity);
        if schema.get("default").is_some() {
            return true;
        }
        if let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| reference.strip_prefix('#'))
            .and_then(|pointer| scope.pointer(pointer).or_else(|| root.pointer(pointer)))
        {
            pending.push((target, scope));
        }
        for key in ["properties", "patternProperties", "dependentSchemas"] {
            if let Some(fields) = schema.get(key).and_then(Value::as_object) {
                pending.extend(fields.values().map(|child| (child, scope)));
            }
        }
        for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
            if let Some(items) = schema.get(key).and_then(Value::as_array) {
                pending.extend(items.iter().map(|child| (child, scope)));
            }
        }
        for key in [
            "items",
            "contains",
            "additionalProperties",
            "unevaluatedProperties",
            "unevaluatedItems",
            "propertyNames",
            "not",
            "if",
            "then",
            "else",
            "contentSchema",
        ] {
            if let Some(child) = schema.get(key) {
                pending.push((child, scope));
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn instance_field_names_and_literals_are_not_schema_defaults() {
        for schema in [
            json!({"properties":{"default":{"const":true}}}),
            json!({"const":{"default":42}}),
            json!({"enum":[{"default":42}]}),
            json!({"examples":[{"default":42}]}),
            json!({"$defs":{"unused":{"default":42}}}),
        ] {
            assert!(!has_default(&schema, &schema), "{schema}");
        }
    }

    #[test]
    fn nested_and_referenced_defaults_are_reachable_without_reference_loops() {
        let root = json!({"$defs": {
            "with_default":{"properties":{"x":{"default":null}}},
            "cycle":{"$ref":"#/$defs/cycle"}
        }});
        for schema in [
            json!({"$ref":"#/$defs/with_default"}),
            json!({"items":{"default":[]}}),
            json!({"anyOf":[{"type":"string"},{"default":0}]}),
            json!({"properties":{"default":{"default":true}}}),
        ] {
            assert!(has_default(&schema, &root), "{schema}");
        }
        assert!(!has_default(&json!({"$ref":"#/$defs/cycle"}), &root));
    }
}
