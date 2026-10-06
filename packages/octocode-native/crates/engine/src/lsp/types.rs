use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JsLanguageServerConfig {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub workspace_root: String,
    pub language_id: Option<String>,
    pub initialization_options: Option<Value>,
    /// Extra environment variables to inject into the language server process.
    pub env: Option<HashMap<String, String>>,
    /// OS-level memory cap for the spawned server process, in MiB. `None`
    /// applies a generous 4 GiB default; `Some(0)` disables the cap for
    /// servers that legitimately need more (huge monorepo indexes). Internal
    /// buffer bounds do not protect the host from a runaway child — this does.
    pub max_memory_mb: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JsExactPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JsRange {
    pub start: JsExactPosition,
    pub end: JsExactPosition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JsCodeSnippet {
    pub uri: String,
    pub range: JsRange,
    pub content: String,
    pub symbol_kind: Option<String>,
    pub display_range: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JsResolvedSymbol {
    pub position: JsExactPosition,
    pub found_at_line: u32,
    pub line_offset: i32,
    pub line_content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JsFuzzyPosition {
    pub symbol_name: String,
    pub line_hint: Option<u32>,
    pub order_hint: Option<u32>,
}
