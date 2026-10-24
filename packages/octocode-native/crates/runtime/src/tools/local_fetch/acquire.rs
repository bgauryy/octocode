//! Opening a read: the path policy, the regular-file and size checks, the
//! binary sniff and the streamed path for large sources.

use super::executor::system_time_iso;
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use std::fs;
use std::io::Read;
use std::path::Path;

/// In-memory read guard. Larger sources use bounded streaming windows up to
/// `MAX_STREAM_SOURCE_BYTES`; a growing small-file read stops at this guard.
pub(crate) const MAX_SOURCE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) enum SourceSizeLimit {
    Streaming,
    InMemoryGrowth,
}

pub(super) fn source_too_large(path: &str, len: u64, limit: SourceSizeLimit) -> LocalFetchResult {
    let (message, hint) = match limit {
        SourceSizeLimit::Streaming => (
            format!(
                "File too large: {len} bytes (streaming ceiling: {} bytes / 1 GiB). localFetch cannot read this path, even in ranges.",
                super::large_source::MAX_STREAM_SOURCE_BYTES
            ),
            "No selection lifts this source-size ceiling; choose a smaller text source or split it outside localFetch.",
        ),
        SourceSizeLimit::InMemoryGrowth => (
            format!(
                "Source grew past the in-memory read guard: at least {len} bytes (guard: {MAX_SOURCE_BYTES} bytes / 10 MiB); the read stopped."
            ),
            "Retry a bounded line window once the source stops changing; sources over 10 MiB stream, up to the 1 GiB ceiling.",
        ),
    };
    let mut result = LocalFetchResult::error(path.to_owned(), ErrorCode::FileTooLarge, message);
    result.source_bytes = Some(len as usize);
    result.terminal_limit(PartialReason::SourceSizeLimit);
    result.hints = vec![hint.into()];
    result
}

/// The rest of `reader` after the `prefix` already read from its start, at
/// most `limit` bytes in all ([`octocode_engine::text::read_source_from`]),
/// checking `cancel` between chunks: a cancel fails as an
/// [`std::io::ErrorKind::Interrupted`] read carrying its reason.
pub(super) fn read_rest(
    reader: impl Read,
    prefix: Vec<u8>,
    limit: u64,
    cancel: &impl CancellationCheck,
) -> Result<Vec<u8>, octocode_engine::text::ReadError> {
    octocode_engine::text::read_source_from(reader, prefix, limit, || {
        cancel
            .check()
            .map_err(|reason| std::io::Error::new(std::io::ErrorKind::Interrupted, reason))
    })
}

/// A failed [`read_rest`] as the row's error: a cancel, or the read failure.
pub(super) fn read_failure(path: &str, error: std::io::Error) -> LocalFetchResult {
    let code = if error.kind() == std::io::ErrorKind::Interrupted {
        ErrorCode::Cancelled
    } else {
        ErrorCode::FileReadFailed
    };
    LocalFetchResult::error(path.to_owned(), code, error.to_string())
}

/// Bytes of the binary sniff: one read from the start of the file.
pub(super) const BINARY_SAMPLE_BYTES: usize = 8192;

/// Name the read file as [`ValidatedRead::named`] does: by the canonical
/// path the read resolved (`.`/`..` folded), relative to the workspace when
/// inside it, or by the requested path when it was read through a symlink;
/// in the row, and in every read that continues the same file, so a
/// continuation names the file its row names. The first page says where a
/// symlink led. Whole-file and streamed reads name it the same way.
pub(super) fn name_read_file(
    result: &mut LocalFetchResult,
    q: &LocalFetchQuery,
    read: &ValidatedRead,
) -> Result<(), octocode_contract_types::error::ConversionError> {
    let named = read.named.as_str();
    if result.path == local_fetch_query::path(q) {
        named.clone_into(&mut result.path);
    }
    if let Some(target) = &read.target
        && result
            .pagination
            .as_ref()
            .is_none_or(|page| page.offset == 0)
    {
        result
            .warnings
            .push(format!("{named} resolves through a symlink to {target}."));
    }
    let Some(next) = result.next.as_mut() else {
        return Ok(());
    };
    for read in next.reads_mut() {
        if local_fetch_query::path(&read.query) == local_fetch_query::path(q) {
            local_fetch_query::set_path(&mut read.query, named)?;
        }
    }
    Ok(())
}

/// The validated path, opened, and the handle's stat; a refused path, a
/// directory, a non-regular file or a failed open ends the read.
pub(super) fn resolve_file(
    q: &LocalFetchQuery,
    paths: &impl PathAccess,
) -> Result<(ValidatedRead, fs::File, fs::Metadata), Box<LocalFetchResult>> {
    let validated = paths
        .validate_read(Path::new(local_fetch_query::path(q)))
        .map_err(|failure| Box::new(path_failure(q, *failure)))?;
    // Open first, then stat the handle: a path swapped for a FIFO or a
    // directory after the policy check is refused, never blocked on. A failed
    // open is named by a plain stat of the path.
    match octocode_engine::text::open_regular(&validated.canonical) {
        Ok((file, meta)) => Ok((validated, file, meta)),
        Err(error) => Err(Box::new(match fs::metadata(&validated.canonical) {
            Ok(meta) if meta.is_dir() => not_a_file(q, NotAFile::Directory),
            Ok(meta) if !meta.is_file() => not_a_file(q, NotAFile::Special),
            _ => LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                ErrorCode::FileAccessFailed,
                error.to_string(),
            ),
        })),
    }
}

/// The opened file's binary-sniff sample, which starts the buffer of the
/// whole read (sized from the handle's stat).
pub(super) fn read_sample(
    mut file: fs::File,
    len: u64,
    in_memory: bool,
) -> std::io::Result<(fs::File, Vec<u8>)> {
    let capacity = if in_memory {
        usize::try_from(len).map_or(BINARY_SAMPLE_BYTES, |len| len + 1)
    } else {
        BINARY_SAMPLE_BYTES
    };
    let mut bytes = Vec::with_capacity(capacity.max(BINARY_SAMPLE_BYTES));
    let mut sample = [0_u8; BINARY_SAMPLE_BYTES];
    let read = file.read(&mut sample)?;
    bytes.extend_from_slice(&sample[..read]);
    Ok((file, bytes))
}

/// A source over the in-memory guard: its streamed line window, ranked
/// when the read has goals.
pub(super) fn fetch_streamed(
    q: &LocalFetchQuery,
    file: &ValidatedRead,
    source: &fs::File,
    meta: &fs::Metadata,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    if local_fetch_query::has_classification(q) {
        return unsupported_selection(
            q,
            "Streamed files cannot use inline selection, even with ranges. Read the unfiltered window, then use clasify with bounded state or source ranges.",
            None,
        );
    }
    if local_fetch_query::selection(q).is_some() {
        return stale_snapshot(q);
    }
    let path = &file.canonical;
    let mut result = super::large_source::fetch_window(
        q,
        path,
        source,
        meta,
        meta.modified().ok().and_then(system_time_iso),
        share,
        security,
        cancel,
    );
    if let Err(error) = name_read_file(&mut result, q, file) {
        return LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::ExecutionFailed,
            error.to_string(),
        );
    }
    result
}

/// A path that names no regular file: a directory leads to its tree; a
/// FIFO, socket or device has no file text to read, so nothing retries it.
pub(super) fn not_a_file(q: &LocalFetchQuery, kind: NotAFile) -> LocalFetchResult {
    if kind == NotAFile::Special {
        return LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::NotAFile,
            format!(
                "{} is not a regular file (a FIFO, socket or device); localFetch reads regular files only.",
                local_fetch_query::path(q)
            ),
        );
    }
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::NotAFile,
        format!(
            "{} is a directory, not a file; list it with structureSearch, then read a file in it.",
            local_fetch_query::path(q)
        ),
    );
    result.next = Some(NextCalls {
        view_tree: Some(
            crate::response::result::Continuation::related(
                ToolId::StructureSearch,
                serde_json::json!({"operation": "tree", "path": local_fetch_query::path(q)}),
            )
            .build(),
        ),
        ..NextCalls::default()
    });
    result
}

/// The row for a path the policy refused or could not resolve.
fn path_failure(q: &LocalFetchQuery, failure: PathFailure) -> LocalFetchResult {
    if let Some(kind) = failure.not_a_file {
        return not_a_file(q, kind);
    }
    let display = failure
        .safe_path
        .as_deref()
        .unwrap_or(local_fetch_query::path(q));
    let message = if failure.sparse_checkout {
        format!(
            "Path does not exist: {display}. It lies in a sparse checkout, so it may exist outside the checked-out paths: re-clone with a path that covers it, or read it with ghGetFileContent."
        )
    } else if failure.resource_missing {
        format!("Path does not exist: {display}")
    } else {
        failure.message
    };
    let mut result =
        LocalFetchResult::error(local_fetch_query::path(q).to_owned(), failure.code, message);
    result.resource_missing = failure.resource_missing;
    if failure.resource_missing {
        // The parent's tree shows a mistyped name's siblings; the listing
        // finds a file moved elsewhere or under another extension; the read
        // of the one close sibling fixes a mistyped name in one call.
        let next = NextCalls {
            view_tree: failure.view_tree,
            find_file: failure.search_root.as_deref().and_then(|root| {
                crate::tools::leads::find_file::find_file(root, local_fetch_query::path(q))
            }),
            read: failure
                .corrected
                .and_then(|path| {
                    local_fetch_query::updated(q, serde_json::json!({"path":path})).ok()
                })
                .map(FetchCall::related),
            ..NextCalls::default()
        };
        result.next = Some(next).filter(|next| !next.is_empty());
    }
    result
}

/// The refusal for a binary file: a NUL byte, or a binary head.
pub(super) fn refuse_binary(q: &LocalFetchQuery, display_path: &str) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::BinaryFileUnsupported,
        format!(
            "Binary file unsupported: {display_path}. Read a text source file, or use structureSearch operation:\"files\" for file metadata."
        ),
    );
    // No range or matchString can make binary bytes readable; say so
    // instead of the generic "remove matchString" recovery.
    result.hints =
        vec!["Do not retry this path with other ranges; choose a text source file instead.".into()];
    result
}
