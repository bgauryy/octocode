//! Complete authorized text for source-coordinate planning, before redaction.
use super::{ClassificationError, ExecutionContext, SourceReader, ToolId};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::ops::Range;
use std::path::PathBuf;

pub struct TextResource {
    pub text: String,
    pub path: PathBuf,
    pub metadata: Value,
    pub local_check: Option<Value>,
    /// What a read of exactly this capture is pinned to: a local file's
    /// snapshot (the digest of its bytes, which a read checks the file
    /// against), a GitHub commit or a URL capture's digest. Kept for the
    /// read lead, whose cursor holds it; a local pin is never shown.
    pub(crate) pin: Option<String>,
}

/// Identity of complete acquisition, independent of each resource's ranges.
pub fn acquisition_key(
    source: &Value,
    dispatcher: &dyn SourceReader,
) -> Result<String, ClassificationError> {
    if let Some(url) = source["url"].as_str() {
        return Ok(format!("url:{url}"));
    }
    let tool = source["tool"]
        .as_str()
        .and_then(ToolId::from_name)
        .ok_or_else(|| {
            ClassificationError::new(
                ErrorCode::InvalidClassificationContext,
                "The source reader is invalid.",
                "Use an authorized text reader.",
            )
        })?;
    let mut query = super::prepare_source(tool, &source["query"], dispatcher)?.into_value();
    // A path the policy resolves is keyed by its file; one it cannot (missing,
    // refused) keeps the requested path, and the capture reports the reader's
    // own failure for it.
    if tool == ToolId::LocalFetch
        && let Ok(validated) = dispatcher.paths().validate_read(std::path::Path::new(
            query["path"].as_str().unwrap_or_default(),
        ))
    {
        query["path"] = json!(validated.canonical);
    }
    if let Some(query) = query.as_object_mut() {
        query.remove("ranges");
    }
    // A field stated at its contract default is its omission.
    octocode_runtime_base::response::result::canonical_row(tool, &mut query);
    Ok(octocode_runtime_base::digest::json_sha256(
        &json!({"tool":tool.as_str(),"query":query}),
    ))
}

pub fn selection_metadata(captured: &TextResource, source: &Value) -> Value {
    let mut metadata = captured.metadata.clone();
    metadata["source"] = source.clone();
    if let Some(tool) = source["tool"].as_str().and_then(ToolId::from_name) {
        metadata["read"] = super::read_lead(tool, &source["query"], captured.pin.as_deref());
    }
    metadata
}

pub fn capture(
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<TextResource, ClassificationError> {
    if let Some(url) = source["url"].as_str() {
        let captured =
            super::url::capture(url, dispatcher, context, dispatcher.text_capture_limit())?;
        let text = captured.state["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let provenance = captured.source.unwrap_or(Value::Null);
        return Ok(TextResource {
            text,
            path: PathBuf::from("capture.txt"),
            metadata: json!({"status":"ready","source":source,"sourceDigest":provenance["sourceDigest"],"coordinateSpace":"capture","expiresAt":provenance["expiresAt"],"read":captured.read}),
            local_check: None,
            pin: provenance["sourceDigest"].as_str().map(str::to_owned),
        });
    }
    let tool = source["tool"]
        .as_str()
        .and_then(ToolId::from_name)
        .filter(|tool| matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent))
        .ok_or_else(|| {
            ClassificationError::new(
                ErrorCode::InvalidClassificationContext,
                "A resource must name an authorized text reader or HTTPS URL.",
                "Use localFetch, ghGetFileContent, or url.",
            )
        })?;
    if !dispatcher.availability().is_available(tool.as_str()) {
        return Err(ClassificationError::new(
            ErrorCode::ClassificationContextUnavailable,
            "The resource reader is disabled in this runtime.",
            "Enable the reader before assessing this source.",
        ));
    }
    let query = super::prepare_source(tool, &source["query"], dispatcher)?;
    let (text, path, snapshot, local_check) = match tool {
        ToolId::LocalFetch => {
            let (text, canonical, snapshot) = dispatcher
                .capture_local_text(&query, context)
                .map_err(|error| local_recovery(error, &query, dispatcher))?;
            let check = json!({"tool":ToolId::LocalFetch.as_str(),"query":{"path":query["path"],"snapshot":snapshot},"canonical":canonical});
            (text, canonical, snapshot, Some(check))
        }
        ToolId::GhGetFileContent => {
            let captured = dispatcher.capture_github_content(
                &query,
                context,
                dispatcher.text_capture_limit(),
            )?;
            let text = match octocode_engine::text::admit(
                &captured.bytes,
                octocode_engine::text::Decoding::Unicode,
            ) {
                Ok(decoded) => decoded.text.into_owned(),
                Err(octocode_engine::text::Refusal::Binary) => {
                    return Err(ClassificationError::new(
                        ErrorCode::ClassificationContextUnavailable,
                        "Section assessment requires text; this resource is binary.",
                        "Use an explicit image asset for image questions.",
                    ));
                }
                Err(octocode_engine::text::Refusal::NotUnicode) => {
                    return Err(ClassificationError::new(
                        ErrorCode::ClassificationContextUnavailable,
                        "The source is not valid UTF-8 text (or BOM-marked UTF-16); no sections were inferred.",
                        "Use a text representation with known encoding.",
                    ));
                }
            };
            (
                text,
                PathBuf::from(query["path"].as_str().unwrap_or_default()),
                captured.resolved_ref,
                None,
            )
        }
        _ => {
            return Err(ClassificationError::new(
                ErrorCode::InvalidClassificationContext,
                "The resource reader is unsupported.",
                "Use localFetch, ghGetFileContent, or url.",
            ));
        }
    };
    let read = super::read_lead(tool, &query, Some(&snapshot));
    // A local snapshot is the digest of the file's bytes: the row names the
    // source by the text a read shows of it instead (the sections plan
    // states it), so no shown digest tests a guess of a hidden value.
    let metadata = if local_check.is_some() {
        json!({"status":"ready","source":source,"read":read})
    } else {
        json!({"status":"ready","source":source,"sourceDigest":snapshot,"read":read})
    };
    Ok(TextResource {
        text,
        path,
        metadata,
        local_check,
        pin: Some(snapshot),
    })
}

/// A local capture failure with the recovery a direct read gives: a missing
/// path leads to its nearest existing directory's tree, and a failure that
/// brings no recovery of its own carries its code's shared hint.
pub(super) fn local_recovery(
    mut error: ClassificationError,
    query: &Value,
    dispatcher: &dyn SourceReader,
) -> ClassificationError {
    if error.leads.is_empty()
        && error.code == ErrorCode::NotFound
        && let Some(Value::Object(leads)) =
            octocode_runtime_base::response::result::missing_path_lead(
                dispatcher.paths(),
                query["path"].as_str().unwrap_or_default(),
            )
    {
        error.leads.extend(leads);
    }
    if error.hints.is_empty()
        && error.leads.is_empty()
        && let Some(hint) = octocode_runtime_base::response::rows::error_code_hint(
            dispatcher.output(ToolId::LocalFetch),
            error.code,
            query,
        )
    {
        error.hints.push(hint.to_owned());
    }
    error
}

/// Whether a retained local source still holds the text captured at its
/// snapshot, at the same file. Its text or its file changed: `staleSnapshot`
/// with the one shared message, as a read pinned to that snapshot gives; the
/// caller names the restart.
pub fn validate_local(
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<(), ClassificationError> {
    let stale = || {
        ClassificationError::with_hints(
            ErrorCode::StaleSnapshot,
            octocode_runtime_base::response::result::STALE_SNAPSHOT_ERROR,
            Vec::new(),
        )
    };
    let query = super::prepare_source(ToolId::LocalFetch, &source["query"], dispatcher)?;
    dispatcher
        .capture_local_text(&query, context)
        .map_err(|error| match error.code {
            ErrorCode::InvalidClassificationContext => ClassificationError::new(
                ErrorCode::ClassificationContextUnavailable,
                "The retained local source identity is invalid.",
                "Start a fresh sections request.",
            ),
            ErrorCode::StaleSnapshot | ErrorCode::SourceMismatch => stale(),
            _ => error,
        })
        .and_then(|(_, canonical, _)| {
            if source["canonical"] == json!(canonical) {
                Ok(())
            } else {
                Err(stale())
            }
        })
}

pub fn selected_ranges(
    text: &str,
    query: &Value,
) -> Result<Vec<Range<usize>>, ClassificationError> {
    let requested = query["ranges"]
        .as_array()
        .filter(|ranges| !ranges.is_empty());
    let Some(requested) = requested else {
        return Ok(if text.is_empty() {
            Vec::new()
        } else {
            std::iter::once(0..text.len()).collect()
        });
    };
    let mut starts = vec![0];
    starts.extend(text.bytes().enumerate().filter_map(|(index, byte)| {
        (byte == b'\n' && index + 1 < text.len()).then_some(index + 1)
    }));
    let mut selected = Vec::new();
    for range in requested {
        let (start, end): (usize, usize) =
            serde_json::from_value::<octocode_contract_types::SourceLineRange>(range.clone())
                .ok()
                .map(|range| {
                    (
                        octocode_engine::num::saturating_usize(range.start_line.get()),
                        octocode_engine::num::saturating_usize(range.end_line.get()),
                    )
                })
                .filter(|(start, end)| start <= end)
                .ok_or_else(|| {
                    ClassificationError::new(
                        ErrorCode::InvalidClassificationContext,
                        "The source contains an invalid line range.",
                        "Use the reader's canonical ranges field.",
                    )
                })?;
        if let Some(&start_byte) = starts.get(start.saturating_sub(1)) {
            let end_byte = starts.get(end).copied().unwrap_or(text.len());
            if start_byte < end_byte {
                selected.push(start_byte..end_byte);
            }
        }
    }
    selected.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in selected {
        if let Some(previous) = merged.last_mut()
            && previous.end >= range.start
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    if merged.is_empty() && !text.is_empty() {
        return Err(ClassificationError::new(
            ErrorCode::ClassificationContextEmpty,
            "The requested resource ranges select no text.",
            "Check the source ranges or omit them to select the complete text.",
        ));
    }
    Ok(merged)
}

#[cfg(test)]
mod tests;
