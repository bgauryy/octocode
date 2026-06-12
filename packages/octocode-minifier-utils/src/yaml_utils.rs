use serde_yaml::{Mapping, Value as YamlValue};

/// Convert a `serde_json::Value` into a YAML string.
///
/// Mirrors the TypeScript `jsonToYamlString`:
///   - Keys can be sorted alphabetically (`sort_keys`)
///   - Priority keys appear first (`keys_priority`)
///   - Multiline strings → YAML block scalars (handled automatically by serde_yaml)
pub fn json_to_yaml_string_inner(
    json: serde_json::Value,
    sort_keys: bool,
    priority_keys: &[String],
) -> String {
    let yaml_val = json_to_yaml_value(json, sort_keys, priority_keys);
    match serde_yaml::to_string(&yaml_val) {
        Ok(s) => s,
        Err(_) => "# YAML conversion failed\n".to_owned(),
    }
}

fn json_to_yaml_value(
    json: serde_json::Value,
    sort_keys: bool,
    priority: &[String],
) -> YamlValue {
    match json {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            if sort_keys || !priority.is_empty() {
                keys.sort_by(|a, b| {
                    let ai = priority.iter().position(|k| k == a);
                    let bi = priority.iter().position(|k| k == b);
                    match (ai, bi) {
                        (Some(x), Some(y)) => x.cmp(&y),
                        (Some(_), None)    => std::cmp::Ordering::Less,
                        (None, Some(_))    => std::cmp::Ordering::Greater,
                        (None, None) => if sort_keys { a.cmp(b) } else { std::cmp::Ordering::Equal },
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
        serde_json::Value::Array(arr) => {
            YamlValue::Sequence(
                arr.into_iter()
                    .map(|v| json_to_yaml_value(v, sort_keys, priority))
                    .collect(),
            )
        }
        serde_json::Value::String(s)  => YamlValue::String(s),
        serde_json::Value::Bool(b)    => YamlValue::Bool(b),
        serde_json::Value::Null       => YamlValue::Null,
        serde_json::Value::Number(n)  => {
            if let Some(i) = n.as_i64() {
                YamlValue::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                YamlValue::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                YamlValue::Number(serde_yaml::Number::from(f))
            } else {
                YamlValue::String(n.to_string())
            }
        }
    }
}
