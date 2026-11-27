//! One authorized reader page of a source selection, and a failed read
//! as the capture's failure.
use super::{SourceReader, prepare_row};
use crate::judgment::ClassificationError;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::rows;
use serde_json::Value;

/// One reader page as the capture needs it.
pub(super) struct Page {
    pub(super) content: String,
    /// The local file snapshot or the resolved commit SHA.
    pub(super) pin: Option<String>,
    /// The reader's own row for the next content page.
    pub(super) next: Option<Value>,
}

/// The capture's failure when the call stopped (`stop`) before its source
/// was captured.
pub(super) fn stopped(
    stop: octocode_runtime_base::execution::ExecutionError,
) -> ClassificationError {
    let (code, hint) = match stop {
        octocode_runtime_base::execution::ExecutionError::Timeout => (
            ErrorCode::Timeout,
            "Retry with more time, or supply the source text as content.",
        ),
        _ => (
            ErrorCode::Cancelled,
            octocode_runtime_base::response::rows::CANCELLED_HINT,
        ),
    };
    ClassificationError::new(
        code,
        "The call stopped before its source was captured.",
        hint,
    )
}

/// One row of tool `id` through the path a direct call takes: its own
/// contract, the input gate, its executor and output sanitization. Returns
/// the row's sealed `data` and the reader's source digest; a failed row is
/// the capture's failure. `progress` is the private progress an issued
/// cursor restored beside the row ([`prepare_row`]).
pub fn read_rows(
    id: ToolId,
    row: &Value,
    progress: Option<&serde_json::Map<String, Value>>,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<(Value, Option<String>), ClassificationError> {
    let prepared = prepare_row(id, row, progress, dispatcher)?;
    let tool = id.as_str();
    context.check().map_err(stopped)?;
    let result = dispatcher.execute(id, &prepared, context).map_err(|stop| {
        ClassificationError::new(
            match stop {
                octocode_runtime_base::execution::ExecutionError::Timeout => ErrorCode::Timeout,
                octocode_runtime_base::execution::ExecutionError::Cancelled => ErrorCode::Cancelled,
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
    let failed = state["results"][0]["status"] == "error";
    let data = state["results"][0]["data"].clone();
    if failed || data.get("errorCode").is_some() {
        return Err(reader_failure(dispatcher.output(id), &data));
    }
    Ok((data, source_digest))
}

pub(super) fn read_page(
    id: ToolId,
    row: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<Page, ClassificationError> {
    let tool = id.as_str();
    let (data, source_digest) = read_rows(id, row, None, dispatcher, context)?;
    let data = &data;
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
    let next = data["next"]["nextPage"]["input"]["queries"][0]
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
pub const RERUN_HINT: &str = "Run the same read directly to inspect it.";

/// The reader's error row as the capture's failure: its code (when
/// declared), its message, and its own recovery. The reader's retry is a
/// fresh source check shown in `related`. Other leads keep their names. A
/// missing path, a refusal or an invalid input keeps the reader's hints or
/// the code's shared recovery. A transient or restart failure offers a re-run hint.
pub fn reader_failure(
    reader: &dyn octocode_runtime_base::response::output::ToolOutput,
    data: &Value,
) -> ClassificationError {
    use octocode_runtime_base::id::error_codes::{ErrorClass, class};
    use octocode_runtime_base::response::result::Continuation;
    let declared = octocode_runtime_base::id::error_code_of(data).map(|code| (code, class(code)));
    let leads: Vec<(String, Value)> = data["next"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, lead)| octocode_runtime_base::contracts::envelope::is_call(lead))
        .filter_map(|(name, lead)| {
            let call = if name == "retry" {
                let tool = ToolId::from_name(lead["tool"].as_str()?)?;
                Continuation::related_input(tool, lead["input"].clone()).build()
            } else {
                lead.clone()
            };
            Some((name.clone(), call))
        })
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
    let final_failure = declared
        .is_some_and(|(_, class)| !matches!(class, ErrorClass::Transient | ErrorClass::Restart));
    if !final_failure {
        hints.push(RERUN_HINT.into());
    } else if hints.is_empty() && leads.is_empty() {
        // Re-running a final failure fails the same way: say the code's fix.
        let hint = match declared {
            Some((code, ErrorClass::CorrectInput)) => {
                rows::error_code_hint(reader, code, &Value::Null)
            }
            Some((code, _)) => {
                rows::error_code_hint(reader, code, &Value::Null).or(Some(RERUN_HINT))
            }
            None => Some(RERUN_HINT),
        };
        hints.extend(hint.map(str::to_owned));
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
