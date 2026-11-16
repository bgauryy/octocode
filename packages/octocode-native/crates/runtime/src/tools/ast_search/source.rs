//! The source astSearch parses: read bounded, admitted as text, and the
//! terminal row (with its text-search reach) of a source past the limit.

use octocode_contract_types::ErrorCode;
use octocode_engine::text::ReadError;
use serde_json::Value;

use super::errors::{io_error, policy_error};
use crate::policy::PolicyError;
use crate::response::result::ToolError;
use crate::tools::display_name;

/// The source astSearch parses, read bounded at the whole-read limit
/// ([`crate::security::SourceLimits::whole_source`]): every parse is
/// bounded by its work and the request, not by size. `None` when the file
/// is larger, or when its parse is estimated to need more memory than the
/// budget ([`octocode_engine::structural::parse_cost`]): a dense one-line
/// file within the byte limit holds as many nodes as a far longer file.
pub(super) fn read_parse_source(path: &std::path::Path) -> Result<Option<Vec<u8>>, ToolError> {
    use crate::tools::source::read_bounded;
    match read_bounded(path, crate::security::SourceLimits::REAL.whole_source) {
        Ok(bytes)
            if octocode_engine::structural::parse_cost(&bytes)
                > octocode_engine::structural::parse_budget() =>
        {
            Ok(None)
        }
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
/// (`Decoding::Lenient`: binary refused, UTF-8, BOM-marked UTF-16, else
/// Latin-1 or lossy UTF-8 with a warning), so a file parses the same named
/// alone or in a directory. Coordinates count the text a read shows of it
/// ([`admitted_for_match`]).
pub(super) fn parse_text<'b>(
    path: &std::path::Path,
    bytes: &'b [u8],
) -> Result<std::borrow::Cow<'b, str>, ToolError> {
    use octocode_engine::text::{Decoding, admit};
    admit(bytes, Decoding::Lenient)
        .map(|decoded| decoded.text)
        .map_err(|refusal| refusal_error(path, refusal))
}

/// [`parse_text`] as a match reads it ([`judge`]): the text a read shows,
/// each placeholder written over in place, so a match, or a line a hint
/// names for one, never depends on a hidden byte.
pub(super) fn matched_text<'b>(
    path: &std::path::Path,
    bytes: &'b [u8],
) -> Result<std::borrow::Cow<'b, str>, ToolError> {
    let text = parse_text(path, bytes)?;
    let hidden = hidden(path, &text);
    if hidden.is_empty() {
        return Ok(text);
    }
    let (matched, _) =
        octocode_engine::text::analysis::AdmittedSource::from_text(&text).judged(&hidden);
    Ok(std::borrow::Cow::Owned(matched.text().to_owned()))
}

/// The spans of `text`, the admitted text of the source at `path`, a read
/// of it withholds, each with the placeholder the read shows there: the
/// source's whole-file judgment, by the pass kept of that text (which the
/// rows' own judging of the same text takes again).
fn hidden(path: &std::path::Path, text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let security = crate::security::ContentSecurity::new();
    let judged = crate::tools::judged_rows::judged_text(text, path, &security);
    judged
        .redaction()
        .map(|(clean, edits)| {
            edits
                .runs(clean)
                .map(|(range, shown)| (range, shown.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// What astSearch parses of every source: the text a read shows, each
/// placeholder written over in place ([`octocode_engine::structural::JudgeText`]),
/// so a match, a count, a `regex` or `range` constraint, a
/// `follows`/`precedes` rule, a position or a digest never depends on a
/// hidden byte or its length: every position counts the text a read shows.
pub(super) fn judge() -> octocode_engine::structural::TextJudge {
    octocode_engine::structural::TextJudge(std::sync::Arc::new(hidden))
}

/// The one error of a source refused as text, whichever reader refused it:
/// binary content, or malformed declared UTF-16.
fn refusal_error(path: &std::path::Path, refusal: octocode_engine::text::Refusal) -> ToolError {
    use octocode_engine::text::Refusal;
    match refusal {
        Refusal::Binary => policy_error(PolicyError::new(
            crate::policy::PolicyErrorCode::BinaryContent,
            format!("{} is binary content, not source text.", display_name(path)),
        )),
        Refusal::NotUnicode => ToolError::new(
            ErrorCode::Decode,
            format!(
                "Could not read {} as source text: {refusal}.",
                display_name(path)
            ),
        ),
    }
}

/// A source-map pass that ends with `cancel`'s request.
pub(super) fn mapping_budget<'a>(
    cancel: &dyn crate::execution::cancel::CancellationCheck,
    cancelled: &'a (dyn Fn() -> bool + Sync),
) -> octocode_engine::text::analysis::MapBudget<'a> {
    octocode_engine::text::analysis::MapBudget::new(
        octocode_engine::text::analysis::MapLimits::default(),
        crate::execution::cancel::deadline_of(cancel),
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

/// [`admitted`] as a match reads it ([`judge`]): the text a read shows, with
/// that shown text when a span was hidden, what the match's texts are
/// restated from ([`octocode_engine::structural::restore_texts`]). Symbols
/// and syntax trees read it too, so every operation's positions, ranges and
/// digests count the shown text.
pub(super) fn admitted_for_match<'a>(
    path: &std::path::Path,
    bytes: &'a [u8],
    budget: &mut octocode_engine::text::analysis::MapBudget<'_>,
) -> Result<
    (
        octocode_engine::text::analysis::AdmittedSource<'a>,
        Option<String>,
    ),
    ToolError,
> {
    let admitted = admitted(path, bytes, budget)?;
    let hidden = hidden(path, admitted.text());
    if hidden.is_empty() {
        return Ok((admitted, None));
    }
    let (matched, shown) = admitted.judged(&hidden);
    Ok((matched, Some(shown)))
}

const TOO_LARGE: &str =
    "Source exceeds the byte limit of a whole read, or the memory its parse is estimated to need.";

/// The terminal row of a source larger than a whole read admits, with its
/// recovery ([`too_large_recovery`]).
pub(super) fn source_limit(
    canonical: &std::path::Path,
    path: &str,
    literal: Option<&str>,
) -> Value {
    let mut row = serde_json::json!({"status":"error","path":path,"errorCode":ErrorCode::FileTooLarge,"error":TOO_LARGE});
    crate::response::paging::terminal_limit(
        &mut row,
        crate::response::paging::PartialReason::FileTooLarge,
        &[],
    );
    let (hints, calls) = too_large_recovery(canonical, path, literal);
    crate::response::result::set_hints(&mut row, hints);
    if let Some(calls) = calls {
        row["calls"] = calls;
    }
    row
}

/// The error of a source larger than a whole read admits, with its
/// recovery ([`too_large_recovery`]).
pub(super) fn too_large_error(
    canonical: &std::path::Path,
    path: &str,
    literal: Option<&str>,
) -> ToolError {
    let mut error = ToolError::new(ErrorCode::FileTooLarge, TOO_LARGE);
    let (hints, calls) = too_large_recovery(canonical, path, literal);
    error.hints = hints;
    error.calls = calls.map(Box::new);
    error
}

/// What recovers a source past the whole-read limit, offered only where the
/// next tool serves the file at its length: within the text-search line
/// ceiling, the shared large-source hint and a search of a searched literal
/// (a symbol name, a pattern without metavariables) in the same file;
/// otherwise, within the streaming ceiling, a windowed read of it. Past both,
/// nothing: every local reader would refuse it too.
fn too_large_recovery(
    canonical: &std::path::Path,
    path: &str,
    literal: Option<&str>,
) -> (Vec<String>, Option<Value>) {
    let len = std::fs::metadata(canonical).map_or(u64::MAX, |meta| meta.len());
    let searchable =
        len <= octocode_engine::search::text_search::DEFAULT_MAX_LINE_SEARCH_FILE_BYTES;
    let readable = len <= crate::tools::source_view::prepare::MAX_STREAM_SOURCE_BYTES;
    let hints = if searchable {
        vec![crate::response::rows::LARGE_SOURCE_HINT.to_owned()]
    } else {
        Vec::new()
    };
    let lead = literal
        .filter(|_| searchable)
        .and_then(|literal| text_search(path, literal))
        .map(|lead| ("findText", lead))
        .or_else(|| {
            readable
                .then(|| window_read(path))
                .flatten()
                .map(|lead| ("read", lead))
        });
    let calls = lead.map(|(name, lead)| serde_json::json!({ name: lead }));
    (hints, calls)
}

/// A localFetch read of `path` from its first window.
fn window_read(path: &str) -> Option<Value> {
    let query: octocode_contract_types::LocalFetchQuery =
        serde_json::from_value(serde_json::json!({ "path": path })).ok()?;
    Some(crate::response::result::Continuation::related_query(query).build())
}

pub(super) fn text_search(path: &str, literal: &str) -> Option<Value> {
    let literal = literal.trim();
    if literal.is_empty() || literal.contains('\n') {
        return None;
    }
    let query = crate::tools::source_view::extraction::text_search_query(path, &[literal])?;
    Some(crate::response::result::Continuation::related_query(query).build())
}

/// A pattern's text when it names no metavariable: the literal a text
/// search can still find.
pub(super) fn pattern_literal(pattern: &str) -> Option<&str> {
    (!pattern.contains('$')).then_some(pattern)
}

#[cfg(test)]
mod tests;
