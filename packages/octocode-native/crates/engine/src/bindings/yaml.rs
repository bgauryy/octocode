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
    crate::portable::json_to_yaml_string(json_object, config)
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
