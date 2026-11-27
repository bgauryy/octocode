use super::{MAX_SOURCE_BYTES, acquire};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::execution::cancel::CancellationCheck;
pub use octocode_tools_shared::source_view::local_fetch_query;
use octocode_tools_shared::source_view::{LocalFetchResult, PathAccess, stale_snapshot};
use std::path::PathBuf;

#[derive(Debug)]
pub struct CapturedText {
    pub text: String,
    pub canonical: PathBuf,
    pub snapshot: String,
}

pub fn capture_text(
    query: &LocalFetchQuery,
    paths: &impl PathAccess,
    cancel: &impl CancellationCheck,
) -> Result<CapturedText, Box<LocalFetchResult>> {
    let failure = |code: ErrorCode, message: String| {
        Box::new(LocalFetchResult::error(
            local_fetch_query::path(query).to_owned(),
            code,
            message,
        ))
    };
    let check = || {
        cancel
            .check()
            .map_err(|error| failure(ErrorCode::Cancelled, error))
    };
    check()?;
    let too_large = || {
        failure(
            ErrorCode::ClassificationContextTooLarge,
            format!(
                "Complete source capture exceeds the {MAX_SOURCE_BYTES}-byte limit; no sections were assessed. Use a smaller text resource."
            ),
        )
    };
    // The capture is one version of the source: a read a write moved while
    // it was under way is made again ([`acquire::settled`]).
    let mut settling = octocode_engine::text::Settling::default();
    let (validated, bytes) = loop {
        let (validated, file, metadata) = acquire::resolve_file(query, paths)?;
        if metadata.len() > MAX_SOURCE_BYTES {
            return Err(too_large());
        }
        let bytes = match acquire::read_rest(&file, Vec::new(), MAX_SOURCE_BYTES, cancel) {
            Ok(bytes) => bytes,
            Err(octocode_engine::text::ReadError::TooLarge(_)) => return Err(too_large()),
            Err(octocode_engine::text::ReadError::Io(error)) => {
                return Err(Box::new(acquire::read_failure(
                    local_fetch_query::path(query),
                    error,
                )));
            }
            Err(error @ octocode_engine::text::ReadError::NotRegular) => {
                return Err(failure(ErrorCode::FileReadFailed, error.to_string()));
            }
        };
        if acquire::settled(query, &mut settling, (&file, &metadata), cancel)? {
            break (validated, bytes);
        }
    };
    check()?;
    let snapshot = octocode_engine::digest::sha256(&bytes);
    let admitted = octocode_engine::text::admit(&bytes, octocode_engine::text::Decoding::Unicode);
    if admitted.as_ref().err() == Some(&octocode_engine::text::Refusal::Binary) {
        return Err(Box::new(octocode_tools_shared::source_view::refuse_binary(
            query,
            &validated.display,
        )));
    }
    if let Some(expected) = local_fetch_query::snapshot(query)
        && expected != snapshot
    {
        return Err(Box::new(stale_snapshot(query)));
    }
    let text = admitted.map_err(|_| {
        failure(
            ErrorCode::ClassificationContextUnavailable,
            "The source is not valid UTF-8 or BOM-marked UTF-16 text; no sections were assessed. Use a text representation with known encoding.".into(),
        )
    })?;
    Ok(CapturedText {
        text: text.text.into_owned(),
        canonical: validated.canonical,
        snapshot,
    })
}

#[cfg(test)]
mod tests;
