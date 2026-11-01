use super::acquire::*;
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::local_fetch_query;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

/// One localFetch row; `selection` judges it under `classification`.
pub fn execute_local_fetch(
    q: &LocalFetchQuery,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    window: Option<usize>,
    selection: Option<&crate::tools::selection::Runtime>,
    views: &std::sync::Arc<ViewCache>,
) -> LocalFetchResult {
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::Cancelled,
            e,
        );
    }
    let (validated, file, meta) = match resolve_file(q, paths) {
        Ok(resolved) => resolved,
        Err(result) => return *result,
    };
    let path = &validated.canonical;
    let display_path = &validated.display;
    // Past the whole-file ceiling, only a streamed line window is served
    // (see `large_source`); past the streaming ceiling nothing is read.
    if meta.len() > super::large_source::MAX_STREAM_SOURCE_BYTES {
        return source_too_large(
            local_fetch_query::path(q),
            meta.len(),
            SourceSizeLimit::Streaming,
        );
    }
    let in_memory = meta.len() <= MAX_SOURCE_BYTES;
    let (file, sample) = match read_sample(file, meta.len(), in_memory) {
        Ok(opened) => opened,
        Err(error) => {
            return LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                ErrorCode::FileReadFailed,
                error.to_string(),
            );
        }
    };
    if octocode_engine::text::sniff(&sample) == octocode_engine::text::BinaryVerdict::Binary {
        return refuse_binary(q, display_path);
    }
    if !in_memory && octocode_engine::text::has_utf16_bom(&sample) {
        let mut result = LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::FileTooLarge,
            format!(
                "BOM-marked UTF-16 sources require a bounded whole-file decode (limit: {MAX_SOURCE_BYTES} bytes); this source is {} bytes.",
                meta.len()
            ),
        );
        result.terminal_limit(PartialReason::SourceSizeLimit);
        result.hints = vec!["Convert a copy to UTF-8 or split the source into smaller text files; line ranges do not lift this decoding limit.".into()];
        return result;
    }
    if !in_memory {
        return fetch_streamed(q, &validated, &file, &meta, window, security, cancel);
    }
    // The read stops past the cap, in case the file grew past the ceiling
    // between the stat above and this read (TOCTOU).
    let bytes = match read_rest(file, sample, MAX_SOURCE_BYTES, cancel) {
        Ok(bytes) => bytes,
        Err(octocode_engine::text::ReadError::TooLarge(len)) => {
            return source_too_large(
                local_fetch_query::path(q),
                len,
                SourceSizeLimit::InMemoryGrowth,
            );
        }
        Err(octocode_engine::text::ReadError::Io(error)) => {
            return read_failure(local_fetch_query::path(q), error);
        }
        Err(error @ octocode_engine::text::ReadError::NotRegular) => {
            return LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                ErrorCode::FileReadFailed,
                error.to_string(),
            );
        }
    };
    if octocode_engine::text::sniff_complete(&bytes) == octocode_engine::text::BinaryVerdict::Binary
    {
        return refuse_binary(q, display_path);
    }
    let mut result = process_fetched_content_selected(
        q,
        &bytes,
        path,
        display_path,
        &SourceFacts {
            views: views.clone(),
            modified: meta.modified().ok().and_then(system_time_iso),
            window,
            beside: 0,
            interior: false,
            byte_origin: None,
        },
        security,
        cancel,
        selection,
    );
    if let Err(error) = bind_continuation_to_source(&mut result)
        .and_then(|()| name_read_file(&mut result, q, &validated))
    {
        return LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::ExecutionFailed,
            error.to_string(),
        );
    }
    result
}

/// Bind pages and recovery of unread or unfiltered evidence to the captured
/// file version, so they cannot silently read a different source.
/// (Remote reads pin a commit SHA in their continuation instead.)
fn bind_continuation_to_source(
    result: &mut LocalFetchResult,
) -> Result<(), octocode_contract_types::error::ConversionError> {
    let snapshot = result.source_sha256.as_deref();
    if let Some(next) = result.next.as_mut() {
        for read in next.reads_of_mut(ReadSource::Page) {
            local_fetch_query::set_snapshot(&mut read.query, snapshot)?;
        }
        if let Some(digest) = result.source_sha256.as_deref() {
            for rows in next
                .unfiltered_unknown_rest
                .values_mut()
                .filter_map(|call| call.pointer_mut("/input/queries"))
                .filter_map(serde_json::Value::as_array_mut)
            {
                for row in rows {
                    row["snapshot"] = serde_json::json!(digest);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn system_time_iso(t: std::time::SystemTime) -> Option<String> {
    let elapsed = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(crate::civil_date::iso8601_millis(
        i64::try_from(elapsed.as_millis()).ok()?,
    ))
}

#[cfg(test)]
mod tests;
