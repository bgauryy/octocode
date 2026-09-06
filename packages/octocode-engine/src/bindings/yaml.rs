use crate::types::YamlConversionConfig;
use napi_derive::napi;

/// Serialize a JSON value to YAML — the formatter for every MCP tool
/// response. Optional key sorting and priority-key ordering; multiline
/// strings become block scalars. Emission is locked by yaml_utils tests.
#[napi(js_name = "jsonToYamlString")]
pub fn json_to_yaml_string(
    json_object: serde_json::Value,
    config: Option<YamlConversionConfig>,
) -> String {
    let sort_keys = config.as_ref().and_then(|c| c.sort_keys).unwrap_or(false);
    let priority_keys = config
        .as_ref()
        .and_then(|c| c.keys_priority.as_deref())
        .map(<[_]>::to_vec)
        .unwrap_or_default();
    crate::text::yaml_utils::json_to_yaml_string_inner(json_object, sort_keys, &priority_keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_when_config_omitted() {
        let out = json_to_yaml_string(json!({"k": "v"}), None);
        assert_eq!(out, "k: v\n");
    }
}
