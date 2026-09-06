use napi_derive::napi;
use std::collections::HashMap;

/// Returns the full MINIFY_CONFIG as a JS-compatible object.
/// Shape: `{ fileTypes: Record<string, { strategy: string, comments: string | string[] | null }> }`
#[napi(js_name = "getMINIFY_CONFIG")]
pub fn get_minify_config() -> serde_json::Value {
    let file_types: HashMap<String, serde_json::Value> = crate::minify::config::minify_config()
        .iter()
        .map(|(ext, cfg)| {
            let comments: serde_json::Value = match cfg.comments {
                None => serde_json::Value::Null,
                Some(groups) if groups.len() == 1 => {
                    serde_json::Value::String(groups[0].to_string())
                }
                Some(groups) => serde_json::Value::Array(
                    groups
                        .iter()
                        .map(|g| serde_json::Value::String((*g).to_string()))
                        .collect(),
                ),
            };
            (
                ext.to_string(),
                serde_json::json!({ "strategy": cfg.strategy, "comments": comments }),
            )
        })
        .collect();
    serde_json::json!({ "fileTypes": file_types })
}
