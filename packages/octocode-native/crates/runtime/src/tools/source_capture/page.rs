//! One authorized reader page of a source selection, and a failed read
//! as the capture's failure.
use super::{SourceReader, prepare_source};
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::response::rows;
use crate::tools::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// One reader page as the capture needs it.
pub(super) struct Page {
    pub(super) content: String,
    /// The local file snapshot or the resolved commit SHA.
    pub(super) pin: Option<String>,
    /// The reader's own row for the next content page.
    pub(super) next: Option<Value>,
}

pub(super) fn read_page(
    id: ToolId,
    row: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<Page, ClassificationError> {
    let prepared = prepare_source(id, row, dispatcher)?;
    let tool = id.as_str();
    context.check().map_err(|stop| {
        let (code, hint) = match stop {
            crate::execution::ExecutionError::Timeout => (
                ErrorCode::Timeout,
                "Retry with more time, or supply the source text as content.",
            ),
            _ => (ErrorCode::Cancelled, crate::response::rows::CANCELLED_HINT),
        };
        ClassificationError::new(
            code,
            "The call stopped before its source was captured.",
            hint,
        )
    })?;
    let result = dispatcher.execute(id, &prepared, context).map_err(|stop| {
        ClassificationError::new(
            match stop {
                crate::execution::ExecutionError::Timeout => ErrorCode::Timeout,
                crate::execution::ExecutionError::Cancelled => ErrorCode::Cancelled,
                _ => ErrorCode::ClassificationContextFailed,
            },
            format!("The {tool} source read could not complete."),
            RERUN_HINT,
        )
    })?;
    let source_digest = result.source_digest.clone();
    let row_value = rows::result_row(0, result.data, result.status);
    let state = rows::envelope(id, vec![row_value]);
    let state = dispatcher
        .security()
        .seal_output(
            state,
            dispatcher.config().resolved.output.redact_emails && id.is_github(),
            || context.check(),
        )
        .map_err(|_| {
            ClassificationError::new(
                ErrorCode::ClassificationContextFailed,
                "The source read's output could not be sanitized.",
                RERUN_HINT,
            )
        })?;
    let data = &state["results"][0]["data"];
    if state["results"][0]["status"] == "error" || data.get("errorCode").is_some() {
        return Err(reader_failure(id, data));
    }
    let content = data["content"].as_str().unwrap_or_default();
    if content.is_empty() {
        return Err(ClassificationError::new(
            ErrorCode::ClassificationContextEmpty,
            format!("The {tool} source selected no text; nothing was judged."),
            "Check the ranges and the path.",
        ));
    }
    let pin = match id {
        ToolId::GhGetFileContent => data["commitSha"]
            .as_str()
            .or_else(|| {
                row["ref"]
                    .as_str()
                    .filter(|reference| octocode_github::is_full_sha(reference))
            })
            .map(str::to_owned),
        _ => source_digest,
    };
    let next = data["next"]["continueWindow"]["input"]["queries"][0]
        .as_object()
        .map(|next| Value::Object(next.clone()));
    Ok(Page {
        content: content.to_owned(),
        pin,
        next,
    })
}

/// Re-running a read that failed for a reason repeating it cannot change
/// (the reader's own hints and leads are the recovery then).
const RERUN_HINT: &str = "Run the same read directly to inspect it.";

/// The reader's error row as the capture's failure: its code (when
/// declared), its message, and its own recovery. The reader's leads
/// (`findFile`, `viewTree`, `repair`, ...) are kept by name. A missing path,
/// a refusal or an invalid input fails the same way on a re-run, so its own
/// hints say what to do (else the code's shared recovery); any other failure
/// also offers the re-run.
fn reader_failure(tool: ToolId, data: &Value) -> ClassificationError {
    use crate::id::error_codes::{ErrorClass, class};
    let declared = crate::id::error_code_of(data).map(|code| (code, class(code)));
    let leads: Vec<(String, Value)> = data["next"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, lead)| crate::response::result::is_call(lead))
        .map(|(name, lead)| (name.clone(), lead.clone()))
        .chain(
            data["related"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, lead)| (format!("related{index}"), lead.clone())),
        )
        .collect();
    let own = data["hints"].as_array().map_or(&[][..], Vec::as_slice);
    let mut hints: Vec<String> = own
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let final_failure = declared.is_some_and(|(_, class)| {
        matches!(
            class,
            ErrorClass::NotFound | ErrorClass::Permission | ErrorClass::InvalidInput
        )
    });
    if !final_failure {
        hints.push(RERUN_HINT.into());
    } else if hints.is_empty() && leads.is_empty() {
        // Re-running a final failure fails the same way: say the code's fix.
        let hint = declared
            .and_then(|(code, _)| crate::response::rows::error_code_hint(tool, code, &Value::Null))
            .unwrap_or(RERUN_HINT);
        hints.push(hint.into());
    }
    ClassificationError {
        leads,
        ..ClassificationError::with_hints(
            declared.map_or(ErrorCode::ClassificationContextFailed, |(code, _)| code),
            data["error"].as_str().unwrap_or("The source read failed."),
            hints,
        )
    }
}

#[cfg(test)]
mod tests;
