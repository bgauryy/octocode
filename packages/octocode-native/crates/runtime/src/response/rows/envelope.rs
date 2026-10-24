use crate::id::ToolId;
use serde_json::{Value, json};

/// Every indexed result owns its paths and evidence.
pub fn envelope(_tool: ToolId, rows: Vec<Value>) -> Value {
    json!({"results":rows})
}
