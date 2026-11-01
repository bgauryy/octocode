use serde::{Deserialize, Serialize};

/// What a graph-facts diagnostic says about its file's facts: consumers
/// branch on the code, never on the message text.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum GraphFactCode {
    /// The static caveat on every tree-sitter file: facts are syntax-only.
    SyntaxOnly,
    /// The parse recovered from syntax errors; facts may be partial.
    RecoveredParse,
    /// An error the JavaScript/TypeScript parser reported (its own text).
    ParseError,
    /// Graph parsing was interrupted by the deadline; facts are incomplete.
    ParseDeadline,
    /// Graph extraction exceeded its deadline; facts are incomplete.
    TraversalDeadline,
    /// Rust macro-generated imports are not linked.
    RustMacroExpansion,
    /// A Rust module carries conditional or custom attributes.
    RustModuleAttributes,
    /// The crate root carries inner conditional or custom attributes.
    RustCrateAttributes,
    /// An observed export-forwarding clause has no admitted member binding.
    UnsupportedExportForwarding,
    /// CommonJS export writes whose final shape is not provable (repeated,
    /// conditional or deferred `module.exports =`, or a rebound `module` or
    /// `exports`): the file exports no CommonJS names.
    UnsupportedCommonJsExports,
}

impl GraphFactCode {
    /// A caveat about import or module linking, which a declaration
    /// listing never does.
    pub fn is_linking_only(self) -> bool {
        matches!(
            self,
            Self::RustMacroExpansion
                | Self::RustModuleAttributes
                | Self::RustCrateAttributes
                | Self::UnsupportedExportForwarding
                | Self::UnsupportedCommonJsExports
        )
    }
}

/// One diagnostic of a graph-facts document.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphFactDiagnostic {
    pub code: GraphFactCode,
    pub message: String,
    /// The 1-based line the diagnostic is about, when it is about one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl std::fmt::Display for GraphFactDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "{} at line {line}", self.message),
            None => f.write_str(&self.message),
        }
    }
}
