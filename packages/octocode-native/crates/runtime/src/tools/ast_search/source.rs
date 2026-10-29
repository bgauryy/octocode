//! The source astSearch parses: read bounded, admitted as text, and the
//! terminal row (with its text-search reach) of a source past the limit.

use octocode_contract_types::ErrorCode;
use octocode_engine::text::ReadError;
use serde_json::Value;

use super::errors::{io_error, policy_error};
use crate::policy::PolicyError;
use crate::response::result::ToolError;
use crate::tools::display_name;

/// The source astSearch parses, read bounded at [`MAX_PARSE_SOURCE_BYTES`];
/// `None` when the file is larger.
pub(super) fn read_parse_source(path: &std::path::Path) -> Result<Option<Vec<u8>>, ToolError> {
    use crate::tools::source::read_bounded;
    match read_bounded(path, MAX_PARSE_SOURCE_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(ReadError::TooLarge(_)) => Ok(None),
        Err(ReadError::NotRegular) => Err(io_error(
            &display_name(path),
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file"),
        )),
        Err(ReadError::Io(error)) => Err(io_error(&display_name(path), error)),
    }
}

/// The text astSearch parses from a source's bytes ([`read_parse_source`]):
/// the engine's one source admission as its directory scan applies it
/// (`Decoding::Unicode`: binary refused, UTF-8 or BOM-marked UTF-16, never
/// a guessed legacy encoding), so a file parses the same named alone or in
/// a directory. Keep the bytes for digests: coordinates count this text.
pub(super) fn parse_text<'b>(
    path: &std::path::Path,
    bytes: &'b [u8],
) -> Result<std::borrow::Cow<'b, str>, ToolError> {
    use octocode_engine::text::{Decoding, admit};
    admit(bytes, Decoding::Unicode)
        .map(|decoded| decoded.text)
        .map_err(|refusal| refusal_error(path, refusal))
}

/// The one error of a source refused as text, whichever reader refused it:
/// binary content, or text that is not Unicode.
fn refusal_error(path: &std::path::Path, refusal: octocode_engine::text::Refusal) -> ToolError {
    use octocode_engine::text::Refusal;
    match refusal {
        Refusal::Binary => policy_error(PolicyError::new(
            crate::policy::PolicyErrorCode::BinaryContent,
            format!("{} is binary content, not source text.", display_name(path)),
        )),
        Refusal::NotUnicode => ToolError::new(
            ErrorCode::FileReadFailed,
            format!(
                "Could not read {} as source text: {refusal}.",
                display_name(path)
            ),
        ),
    }
}

pub(super) fn mapping_budget<'a>(
    cancelled: &'a (dyn Fn() -> bool + Sync),
) -> octocode_engine::text::analysis::MapBudget<'a> {
    octocode_engine::text::analysis::MapBudget::new(
        octocode_engine::text::analysis::MapLimits::default(),
        std::time::Instant::now() + crate::execution::DEFAULT_TIMEOUT,
        cancelled,
    )
}

pub(super) fn admitted<'a>(
    path: &std::path::Path,
    bytes: &'a [u8],
    budget: &mut octocode_engine::text::analysis::MapBudget<'_>,
) -> Result<octocode_engine::text::analysis::AdmittedSource<'a>, ToolError> {
    use octocode_engine::text::analysis::{AdmittedSource, SourceError};
    AdmittedSource::from_bytes(bytes, &path.to_string_lossy(), budget).map_err(
        |error| match error {
            SourceError::Refused(refusal) => refusal_error(path, refusal),
            error => ToolError::source(error),
        },
    )
}

/// The terminal row of a source larger than [`MAX_PARSE_SOURCE_BYTES`]; a
/// searched literal (a symbol name, a pattern without metavariables) leads
/// to a text search of it in the same file.
pub(super) fn source_limit(path: &str, literal: Option<&str>) -> Value {
    let mut row = serde_json::json!({"status":"error","path":path,"errorCode":ErrorCode::FileTooLarge,"error":"Source exceeds the native parser byte limit."});
    crate::response::paging::terminal_limit(&mut row, "fileTooLarge", &[]);
    if let Some(lead) = literal.and_then(|literal| text_search(path, literal)) {
        row["calls"] = serde_json::json!({ "findText": lead });
    }
    row
}

pub(super) fn text_search(path: &str, literal: &str) -> Option<Value> {
    let literal = literal.trim();
    if literal.is_empty() || literal.contains('\n') {
        return None;
    }
    let row = serde_json::json!({"path": path, "matchString": [literal]});
    Some(
        crate::response::result::Continuation::related(crate::id::ToolId::LocalSearch, row).build(),
    )
}

/// A pattern's text when it names no metavariable: the literal a text
/// search can still find.
pub(super) fn pattern_literal(pattern: &str) -> Option<&str> {
    (!pattern.contains('$')).then_some(pattern)
}
/// Largest source astSearch parses (match, symbols, syntaxTree). Sized for
/// real generated and monolithic sources (TypeScript's 3 MB checker.ts, the
/// 2.3 MB lib.dom.d.ts); tree-sitter and oxc parse these in well under a
/// second. Directory scans skip larger files and report them.
pub(crate) const MAX_PARSE_SOURCE_BYTES: usize = octocode_engine::signatures::MAX_PARSE_SIZE;
