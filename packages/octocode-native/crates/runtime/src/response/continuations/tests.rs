use super::compact::compact_query;
use super::*;
use serde_json::Map;
use serde_json::json;
mod inheritance;
mod validity;

fn call(tool: &str, row: Value) -> Value {
    json!({"tool": tool, "query": {"queries": [row]}})
}

fn run(out: &mut Value, tool: ToolId, rows: &[Option<&Value>]) {
    finalize(out, tool, &Sources::Rows(rows), &Scope::everything()).expect("valid calls");
}
