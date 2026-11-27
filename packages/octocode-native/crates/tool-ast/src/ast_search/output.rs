//! astSearch's answers to the shared response stages.

use octocode_contract_types::ErrorCode;
use octocode_runtime_base::response::output::ToolOutput;
use serde_json::Value;

pub struct Output;
impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        Some(match code {
            ErrorCode::InvalidPattern => octocode_tools_shared::ast_rule::INVALID_PATTERN_HINT,
            // The typed error and terminalLimit (plus a findText lead
            // when a literal was searched) are the whole answer.
            ErrorCode::FileTooLarge => octocode_runtime_base::response::output::NO_HINT,
            ErrorCode::LanguageRequired => {
                "Set parser to the grammar of the source files (e.g. \"typescript\", \"rust\")."
            }
            ErrorCode::LanguageUnsupported => {
                "Use localFetch to read the file or localSearch to search its text. Set parser only when the source is written in a supported grammar."
            }
            // The message names the file's own grammar and both repairs.
            ErrorCode::LanguageMismatch => octocode_runtime_base::response::output::NO_HINT,
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
pub(super) fn unsupported_read(path: &str) -> Option<Value> {
    use octocode_tools_shared::source_view::local_fetch_query::{outline_lead, read_lead};
    let read = if octocode_runtime_base::content::DocumentFormat::of(path).is_some() {
        outline_lead(path)
    } else {
        read_lead(path)
    }?;
    Some(octocode_runtime_base::response::result::Continuation::related_query(read).build())
}
