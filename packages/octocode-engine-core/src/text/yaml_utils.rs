use serde_yaml_ng::{Mapping, Value as YamlValue};

/// Convert a `serde_json::Value` into a YAML string.
///
/// Mirrors the TypeScript `jsonToYamlString`:
///   - Keys can be sorted alphabetically (`sort_keys`)
///   - Priority keys appear first (`keys_priority`)
///   - Multiline strings → YAML block scalars (handled automatically by the serializer)
pub fn json_to_yaml_string_inner(
    json: serde_json::Value,
    sort_keys: bool,
    priority_keys: &[String],
) -> String {
    let yaml_val = json_to_yaml_value(json, sort_keys, priority_keys);
    match serde_yaml_ng::to_string(&yaml_val) {
        Ok(s) => s,
        Err(_) => "# YAML conversion failed\n".to_owned(),
    }
}

fn json_to_yaml_value(json: serde_json::Value, sort_keys: bool, priority: &[String]) -> YamlValue {
    match json {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            if sort_keys || !priority.is_empty() {
                keys.sort_by(|a, b| {
                    let ai = priority.iter().position(|k| k == a);
                    let bi = priority.iter().position(|k| k == b);
                    match (ai, bi) {
                        (Some(x), Some(y)) => x.cmp(&y),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => {
                            if sort_keys {
                                a.cmp(b)
                            } else {
                                std::cmp::Ordering::Equal
                            }
                        }
                    }
                });
            }
            let mut mapping = Mapping::new();
            for key in keys {
                if let Some(val) = map.get(&key) {
                    mapping.insert(
                        YamlValue::String(key),
                        json_to_yaml_value(val.clone(), sort_keys, priority),
                    );
                }
            }
            YamlValue::Mapping(mapping)
        }
        serde_json::Value::Array(arr) => YamlValue::Sequence(
            arr.into_iter()
                .map(|v| json_to_yaml_value(v, sort_keys, priority))
                .collect(),
        ),
        serde_json::Value::String(s) => YamlValue::String(s),
        serde_json::Value::Bool(b) => YamlValue::Bool(b),
        serde_json::Value::Null => YamlValue::Null,
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                YamlValue::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                YamlValue::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                YamlValue::Number(serde_yaml_ng::Number::from(f))
            } else {
                YamlValue::String(n.to_string())
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn emit(v: serde_json::Value) -> String {
        json_to_yaml_string_inner(v, false, &[])
    }

    #[test]
    fn keys_sorted_when_sort_keys_enabled() {
        let out = json_to_yaml_string_inner(json!({"z": 1, "a": 2}), true, &[]);
        let a = out.find("a:").expect("a key present");
        let z = out.find("z:").expect("z key present");
        assert!(a < z);
    }

    #[test]
    fn priority_keys_emitted_first() {
        let out = json_to_yaml_string_inner(
            json!({"c": 3, "a": 1, "b": 2}),
            false,
            &["b".to_owned(), "c".to_owned()],
        );
        let b = out.find("b:").expect("b present");
        let c = out.find("c:").expect("c present");
        let a = out.find("a:").expect("a present");
        assert!(b < c && c < a);
    }

    #[test]
    fn multiline_strings_use_block_scalars_with_sorted_keys() {
        let out = json_to_yaml_string_inner(
            json!({"z": "last", "a": "first\nsecond line", "b": 42, "c": true}),
            true,
            &[],
        );
        let a = out.find("a:").expect("a");
        let b = out.find("b:").expect("b");
        let c = out.find("c:").expect("c");
        let z = out.find("z:").expect("z");
        assert!(a < b && b < c && c < z, "keys not sorted: {out}");
        assert!(
            out.contains("|-"),
            "multiline must use a block scalar: {out}"
        );
    }

    /// Emission contract captured empirically from the serde_yaml 0.9 output
    /// that every MCP tool response is built on. Any YAML-crate change must
    /// reproduce these byte-exactly.
    #[test]
    fn emission_contract_locked_across_serializer_changes() {
        assert_eq!(emit(json!({"v": "no"})), "v: no\n");
        assert_eq!(emit(json!({"v": "true"})), "v: 'true'\n");
        assert_eq!(emit(json!({"v": "123"})), "v: '123'\n");
        assert_eq!(emit(json!({"v": "null"})), "v: 'null'\n");
        assert_eq!(emit(json!({"v": ""})), "v: ''\n");
        assert_eq!(emit(json!({"v": "a: b"})), "v: 'a: b'\n");
        assert_eq!(emit(json!({"v": "- item"})), "v: '- item'\n");
        assert_eq!(emit(json!({"v": "#comment"})), "v: '#comment'\n");
        assert_eq!(
            emit(json!({"v": "line1\nline2"})),
            "v: |-\n  line1\n  line2\n"
        );
        assert_eq!(
            emit(json!({"v": "café ünïcode 中文"})),
            "v: café ünïcode 中文\n"
        );
        assert_eq!(emit(json!({"n": 1.5})), "n: 1.5\n");
        assert_eq!(emit(json!({"n": 0})), "n: 0\n");
        assert_eq!(
            emit(json!({"a": [1, "x"], "b": {"c": null}})),
            "a:\n- 1\n- x\nb:\n  c: null\n"
        );
    }
}
