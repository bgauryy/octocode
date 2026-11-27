use super::acquire::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::security::scan::ContentScan;
pub use octocode_tools_shared::source_view::local_fetch_query;
use octocode_tools_shared::source_view::*;

/// One localFetch row. `clasify`: clasify is available, so a refused image
/// names its route there.
pub fn execute_local_fetch(
    q: &LocalFetchQuery,
    paths: &impl PathAccess,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    window: Option<usize>,
    clasify: bool,
) -> LocalFetchResult {
    if let Err(e) = cancel.check() {
        return LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::Cancelled,
            e,
        );
    }
    let limits = security.limits();
    // What is served is one version of the source: a read a write moved
    // while it was under way (a save in place may join the old version's
    // start to the new one's rest) is made again ([`settled`]), unless the
    // source only grew and the read is the start of one version of it
    // ([`settled_read`]).
    let mut settling = octocode_engine::text::Settling::default();
    let (validated, bytes, grown) = loop {
        let (validated, file, meta) = match resolve_file(q, paths) {
            Ok(resolved) => resolved,
            Err(result) => return *result,
        };
        let display_path = &validated.display;
        // Past the whole-file ceiling, only a streamed line window is served
        // (see `large_source`); past the streaming ceiling nothing is read.
        if meta.len() > octocode_tools_shared::large_source::MAX_STREAM_SOURCE_BYTES {
            return source_too_large(
                local_fetch_query::path(q),
                meta.len(),
                SourceSizeLimit::Streaming,
            );
        }
        let in_memory = meta.len() <= limits.whole_source_bytes();
        let (file, mut sample) = match read_sample(file) {
            Ok(opened) => opened,
            Err(error) => {
                return LocalFetchResult::error(
                    local_fetch_query::path(q).to_owned(),
                    ErrorCode::FileReadFailed,
                    error.to_string(),
                );
            }
        };
        #[cfg(test)]
        after_sample(&validated.canonical);
        if octocode_engine::text::sniff(&sample) == octocode_engine::text::BinaryVerdict::Binary {
            return refuse_binary_with_head(q, display_path, clasify_image(clasify, &sample));
        }
        if !in_memory && octocode_engine::text::has_utf16_bom(&sample) {
            let mut result = LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                ErrorCode::FileTooLarge,
                format!(
                    "BOM-marked UTF-16 sources require a bounded whole-file decode (limit: {} bytes); this source is {} bytes.",
                    limits.whole_source,
                    meta.len()
                ),
            );
            result.terminal_limit(PartialReason::FileTooLarge);
            result.hints = vec!["Convert a copy to UTF-8 or split the source into smaller text files; line ranges do not lift this decoding limit.".into()];
            return result;
        }
        if !in_memory {
            let result = fetch_streamed(q, &validated, &file, &meta, window, security, cancel);
            match settled(q, &mut settling, (&file, &meta), cancel) {
                Ok(true) => return result,
                Ok(false) => continue,
                Err(result) => return *result,
            }
        }
        // Reserve the whole-file buffer only after the binary sniff accepted it.
        let capacity = usize::try_from(meta.len()).map_or(BINARY_SAMPLE_BYTES, |len| len + 1);
        sample.reserve(capacity.saturating_sub(sample.len()));
        // The read stops past the cap, in case the file grew past the ceiling
        // between the stat above and this read (TOCTOU).
        let bytes = match read_rest(&file, sample, limits.whole_source_bytes(), cancel) {
            Ok(bytes) => bytes,
            Err(octocode_engine::text::ReadError::TooLarge(len)) => {
                return source_too_large(
                    local_fetch_query::path(q),
                    len,
                    SourceSizeLimit::InMemoryGrowth(limits.whole_source_bytes()),
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
        match settled_read(q, &mut settling, (&file, &meta), &bytes, cancel) {
            Ok(WholeRead::Whole) => break (validated, bytes, false),
            Ok(WholeRead::Grown) => break (validated, bytes, true),
            Ok(WholeRead::Again) => {}
            Err(result) => return *result,
        }
    };
    let path = &validated.canonical;
    let display_path = &validated.display;
    if octocode_engine::text::sniff_complete(&bytes) == octocode_engine::text::BinaryVerdict::Binary
    {
        return refuse_binary_with_head(q, display_path, clasify_image(clasify, &bytes));
    }
    // The first page reads under the name its row and cursor carry, so every
    // page of a walk is one read whatever path the caller passed.
    let mut named_query = q.clone();
    if let Err(error) = local_fetch_query::set_path(&mut named_query, &validated.named) {
        return LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::ExecutionFailed,
            error.to_string(),
        );
    }
    let mut result = process_fetched_content_as(
        &named_query,
        &bytes,
        path,
        display_path,
        &SourceFacts {
            window,
            beside: 0,
            interior: false,
            byte_origin: None,
            file_lines: None,
            local_file: true,
        },
        security,
        cancel,
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
    if grown {
        note_grown(&mut result, bytes.len());
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
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// Whether a binary read's `head` is an image clasify can judge, when clasify
/// is available: the refusal then names its `assets` route.
fn clasify_image(clasify: bool, head: &[u8]) -> bool {
    clasify && octocode_tools_shared::source_capture::assets::is_judgeable_image(head)
}
