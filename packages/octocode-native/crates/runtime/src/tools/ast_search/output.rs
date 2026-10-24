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
            ErrorCode::LanguageRequired | ErrorCode::LanguageUnsupported => {
                "Set parser to the grammar of the source files (e.g. \"typescript\", \"rust\")."
            }
            // The message names the file's own grammar and both repairs.
            ErrorCode::LanguageMismatch => crate::response::output::NO_HINT,
            ErrorCode::SymbolsUnsupported => {
                "Set parser (a file) or parserGlobs (a directory) to the grammar the source is written in."
            }
            ErrorCode::LanguageFileRequired => {
                "For a directory, drop parser: each extension picks its grammar; parserGlobs overrides (e.g. {cpp:[\"**/*.h\"]})."
            }
            ErrorCode::LanguageDirectoryRequired => {
                "For a single file, use parser instead of parserGlobs."
            }
            _ => return None,
        })
    }
}

/// Publish explicitly typed producer calls in their canonical channels.
pub(super) fn finish(mut data: Value) -> Value {
    if let Some(calls) = data.as_object_mut().and_then(|row| row.remove("calls")) {
        crate::response::result::attach_calls(&mut data, calls);
    }
    data
}
