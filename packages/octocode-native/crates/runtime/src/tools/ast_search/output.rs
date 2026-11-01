//! astSearch's answers to the shared response stages.

use crate::response::output::ToolOutput;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        Some(match code {
            ErrorCode::InvalidPattern => crate::tools::ast_rule::INVALID_PATTERN_HINT,
            // The typed error and terminalLimit (plus a findText lead
            // when a literal was searched) are the whole answer.
            ErrorCode::FileTooLarge => crate::response::output::NO_HINT,
            ErrorCode::LanguageRequired => {
                "Set parser to the grammar of the source files (e.g. \"typescript\", \"rust\")."
            }
            ErrorCode::LanguageUnsupported => {
                "Use localFetch to read the file or localSearch to search its text. Set parser only when the source is written in a supported grammar."
            }
            // The message names the file's own grammar and both repairs.
            ErrorCode::LanguageMismatch => crate::response::output::NO_HINT,
            ErrorCode::SymbolsUnsupported => {
                "Use localFetch view:symbols for document headings/keys or localSearch for text. Set parser or parserGlobs only for source written in a supported grammar."
            }
            ErrorCode::LanguageDirectoryRequired => {
                "For a single file, use parser instead of parserGlobs."
            }
            _ => return None,
        })
    }
}

/// An unsupported structural operation can still inspect document headings or
/// keys; other text gets an exact read. The format registry owns that choice.
pub(super) fn unsupported_read(path: &str) -> Value {
    let mut read = serde_json::json!({"path":path});
    if crate::content::DocumentFormat::of(path).is_some() {
        read["view"] = serde_json::json!("symbols");
    }
    crate::response::result::Continuation::related(crate::id::ToolId::LocalFetch, read).build()
}

/// Publish explicitly typed producer calls in their canonical channels.
pub(super) fn finish(mut data: Value) -> Value {
    crate::response::result::finish_calls(&mut data);
    data
}
