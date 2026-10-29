use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspLanguageServerConfig {
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
    /// Where discovery found this launch; receipts report it. Not part of
    /// the pool key: one launch has one source.
    #[serde(default)]
    pub source: ServerSource,
}

/// Where a discovered language-server launch came from.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerSource {
    /// A user `lsp-servers.json` entry (the octocode home's, or the file
    /// `lsp.configPath` names).
    User,
    /// A trusted project's `.octocode/lsp-servers.json` entry.
    Project,
    /// An octocode-managed install (`OCTOCODE_LSP_CACHE_DIR`, else
    /// `<octocode home>/lsp`).
    Managed,
    /// octocode's own install tree (the bundled tsgo, or its
    /// `typescript-language-server`).
    Bundled,
    /// The workspace's or the invocation directory's own `node_modules`.
    Workspace,
    /// The command as `PATH` resolves it.
    #[default]
    Path,
}

impl ServerSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Managed => "managed",
            Self::Bundled => "bundled",
            Self::Workspace => "workspace",
            Self::Path => "path",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspExactPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspRange {
    pub start: LspExactPosition,
    pub end: LspExactPosition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspCodeSnippet {
    pub uri: String,
    pub range: LspRange,
    pub content: String,
    pub symbol_kind: Option<String>,
    pub display_range: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspResolvedSymbol {
    pub position: LspExactPosition,
    pub found_at_line: u32,
    pub line_offset: i32,
    pub line_content: String,
    /// Zero-based UTF-16 columns of every identifier occurrence of the name
    /// on the found line, in `occurrence` order, when the grammar resolved
    /// it; more than one means the line is ambiguous. Empty when unknown.
    #[serde(default)]
    pub occurrences_on_line: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LspFuzzyPosition {
    pub symbol_name: String,
    pub line_hint: Option<u32>,
    pub order_hint: Option<u32>,
}
