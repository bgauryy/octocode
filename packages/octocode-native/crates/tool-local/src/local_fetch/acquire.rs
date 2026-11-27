//! Opening a read: the path policy, the regular-file and size checks, the
//! binary sniff and the streamed path for large sources.

use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::security::scan::ContentScan;
use octocode_tools_shared::source_view::*;
use std::fs;
use std::io::Read;
use std::path::Path;

/// In-memory read guard: what the scanner judges at once, so every source read
/// whole is judged whole. Larger sources use bounded streaming windows up to
/// `MAX_STREAM_SOURCE_BYTES`; a growing small-file read stops at this guard.
pub const MAX_SOURCE_BYTES: u64 = octocode_engine::security::MAX_CONTENT_SIZE as u64;

#[derive(Clone, Copy)]
pub(super) enum SourceSizeLimit {
    Streaming,
    /// A source that grew past the in-memory guard it was read under.
    InMemoryGrowth(u64),
}

pub(super) fn source_too_large(path: &str, len: u64, limit: SourceSizeLimit) -> LocalFetchResult {
    let (message, hint) = match limit {
        SourceSizeLimit::Streaming => (
            format!(
                "File too large: {len} bytes (streaming ceiling: {} bytes / 1 GiB). localFetch cannot read this path, even in ranges.",
                octocode_tools_shared::large_source::MAX_STREAM_SOURCE_BYTES
            ),
            "No selection lifts this source-size ceiling; choose a smaller text source or split it outside localFetch.".to_owned(),
        ),
        SourceSizeLimit::InMemoryGrowth(guard) => {
            let mib = guard / (1024 * 1024);
            (
                format!(
                    "Source grew past the in-memory read guard: at least {len} bytes (guard: {guard} bytes / {mib} MiB); the read stopped."
                ),
                format!(
                    "Retry a bounded line window once the source stops changing; sources over {mib} MiB stream, up to the 1 GiB ceiling."
                ),
            )
        }
    };
    let mut result = LocalFetchResult::error(path.to_owned(), ErrorCode::FileTooLarge, message);
    result.terminal_limit(PartialReason::FileTooLarge);
    result.hints = vec![hint];
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

/// Whether the read of `file` (opened as `meta`) is of one version of it
/// ([`octocode_engine::text::Settling`]): `Ok(true)` when no write moved it,
/// `Ok(false)` to read the source again from a fresh open, else the row a
/// source that keeps changing, or a cancelled call, answers.
pub(super) fn settled(
    q: &LocalFetchQuery,
    settling: &mut octocode_engine::text::Settling,
    (file, meta): (&fs::File, &fs::Metadata),
    cancel: &impl CancellationCheck,
) -> Result<bool, Box<LocalFetchResult>> {
    match settling.settled(file, meta) {
        octocode_engine::text::Settled::Stable(_) => Ok(true),
        octocode_engine::text::Settled::Again => read_again(q, cancel).map(|()| false),
        octocode_engine::text::Settled::Unsettled => Err(Box::new(kept_changing(q))),
    }
}

/// What [`settled_read`] proved of a whole read of a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WholeRead {
    /// It is one version of the source, whole.
    Whole,
    /// It is the start of one version of a source that grew while it was
    /// read: served as of its length, saying so ([`note_grown`]).
    Grown,
    /// Read the source again from a fresh open.
    Again,
}

/// What the whole read of `file` (opened as `meta`) that took `bytes` from
/// its start proved ([`octocode_engine::text::Settling::settled_read`]): a
/// source that only grew while it was read (a live log) is served as of the
/// bytes read when they are the start of one version of it. Else as
/// [`settled`].
pub(super) fn settled_read(
    q: &LocalFetchQuery,
    settling: &mut octocode_engine::text::Settling,
    (file, meta): (&fs::File, &fs::Metadata),
    bytes: &[u8],
    cancel: &impl CancellationCheck,
) -> Result<WholeRead, Box<LocalFetchResult>> {
    use octocode_engine::text::Proved;
    let live = || cancel.check().is_ok();
    let taken = || Some(octocode_engine::text::Taken::Bytes(bytes));
    match settling.settled_read(file, meta, taken, &live) {
        Proved::Whole(_) => Ok(WholeRead::Whole),
        Proved::Prefix(_) => Ok(WholeRead::Grown),
        Proved::Again => read_again(q, cancel).map(|()| WholeRead::Again),
        Proved::Unsettled => Err(Box::new(kept_changing(q))),
    }
}

/// States on `result` that its source grew while it was read: what it
/// shows is the start of one version, as of `len` bytes.
pub(super) fn note_grown(result: &mut LocalFetchResult, len: usize) {
    result.warnings.push(format!(
        "The file grew while it was read: this is its text as of {}, the start of one version of it; text written since is not shown.",
        octocode_runtime_base::response::pages::counted(len as u64, "byte", "bytes")
    ));
}

/// `Ok` to read the source again, or the row of a cancelled call.
fn read_again(
    q: &LocalFetchQuery,
    cancel: &impl CancellationCheck,
) -> Result<(), Box<LocalFetchResult>> {
    cancel.check().map_err(|reason| {
        Box::new(LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::Cancelled,
            reason,
        ))
    })
}

/// The row of a source that kept changing while it was read: no read of it
/// is one version of it, so nothing of it is served.
pub(super) fn kept_changing(q: &LocalFetchQuery) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::FileReadFailed,
        "The file kept changing while it was read, so no read of it is one version of it; nothing of it is served.".to_owned(),
    );
    result.hints = vec![octocode_engine::text::CHANGING_HINT.to_owned()];
    result
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
    let first_page = result
        .pagination
        .as_ref()
        .is_none_or(|page| page.offset == 0);
    if let Some(target) = &read.target
        && first_page
    {
        result
            .warnings
            .push(format!("{named} resolves through a symlink to {target}."));
    }
    if let Some(requested) = &read.repeated
        && first_page
    {
        result.warnings.push(format!(
            "read path as {named}: {requested} repeats a folder and does not exist."
        ));
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
    let validated = match paths.validate_read(Path::new(local_fetch_query::path(q))) {
        Ok(validated) => validated,
        // A missing path that repeats a run of its folders (a row's path
        // joined onto the folder it already names) reads the one path that
        // exists without the repeat; the row says how it was read.
        Err(failure) => match failure
            .meant
            .as_deref()
            .filter(|_| failure.resource_missing)
            .map(|meant| paths.validate_read(Path::new(meant)))
        {
            Some(Ok(mut validated)) => {
                validated.repeated = Some(
                    failure
                        .safe_path
                        .clone()
                        .unwrap_or_else(|| local_fetch_query::path(q).to_owned()),
                );
                validated
            }
            _ => return Err(Box::new(path_failure(q, *failure))),
        },
    };
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

/// Tests: a write made after a read takes its binary-sniff sample of a
/// source and before it reads the rest, for a canonical source path; it
/// returns whether it stays registered for the next read.
#[cfg(test)]
pub(super) type SampleRace = (std::path::PathBuf, Box<dyn FnMut() -> bool + Send>);

/// Tests: the writes [`after_sample`] makes.
#[cfg(test)]
pub(super) static AFTER_SAMPLE: std::sync::Mutex<Vec<SampleRace>> =
    std::sync::Mutex::new(Vec::new());

/// Tests: makes the write registered for `path` in [`AFTER_SAMPLE`].
#[cfg(test)]
pub(super) fn after_sample(path: &Path) {
    let mut races = AFTER_SAMPLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(at) = races.iter().position(|(source, _)| source == path)
        && !(races[at].1)()
    {
        drop(races.remove(at));
    }
}

/// The opened file's binary-sniff sample. Reserve only the sniff until the
/// executor admits the source as text and chooses a whole-file read.
pub(super) fn read_sample(mut file: fs::File) -> std::io::Result<(fs::File, Vec<u8>)> {
    let mut bytes = Vec::with_capacity(BINARY_SAMPLE_BYTES);
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
    let path = &file.canonical;
    let mut result = octocode_tools_shared::large_source::fetch_window(
        q, path, source, meta, share, security, cancel,
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
        view_tree: directory_tree(local_fetch_query::path(q)),
        ..NextCalls::default()
    });
    result
}

/// The structureSearch tree of the directory a read named.
fn directory_tree(path: &str) -> Option<serde_json::Value> {
    use octocode_contract_types::{
        StructureSearchQueryTree, StructureSearchQueryTreeOperation, StructureSearchView,
    };
    let query = StructureSearchQueryTree {
        default_excludes: None,
        entry_type: None,
        exclude: Vec::new(),
        extensions: Vec::new(),
        include: Vec::new(),
        include_hidden: None,
        include_ignored: None,
        limit: None,
        max_depth: std::num::NonZeroU64::MIN,
        operation: StructureSearchQueryTreeOperation::Tree,
        page: std::num::NonZeroU64::MIN,
        path: path.try_into().ok()?,
        scan_offset: None,
        snapshot: None,
        view: StructureSearchView::Details,
        walk_offset: None,
    };
    Some(octocode_runtime_base::response::result::Continuation::related_query(query).build())
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
    // A path a recorded sparse scope leaves out is checked out by the
    // related ghCloneRepo call its message names.
    let reclone = failure
        .clone_scope
        .as_ref()
        .zip(failure.resolved.as_ref())
        .and_then(|(scope, path)| scope.reclone(path));
    let message = if let Some(scope) = failure.clone_scope.as_ref().filter(|_| reclone.is_some()) {
        scope.outside_message(display)
    } else if failure.sparse_checkout {
        format!(
            "Path does not exist: {display}. It lies in a sparse checkout, so it may exist outside the checked-out paths: re-clone with a path that covers it, or read it with ghGetFileContent."
        )
    } else if let Some(bare) = failure
        .corrected
        .as_deref()
        .filter(|_| failure.listing_entry)
    {
        format!(
            "Path does not exist: {display}. {}",
            octocode_tools_shared::root_read::listing_entry_note(bare)
        )
    } else if failure.resource_missing {
        let meant = failure
            .meant
            .as_deref()
            .map(octocode_runtime_base::policy::path::repeated_folder_note);
        format!(
            "Path does not exist: {display}{}",
            meant.unwrap_or_default()
        )
    } else {
        failure.message
    };
    let mut result =
        LocalFetchResult::error(local_fetch_query::path(q).to_owned(), failure.code, message);
    result.resource_missing = failure.resource_missing;
    // A name several entries share has no missing path, but the parent's
    // listing still shows those entries, so a failure that carries one gets it.
    if failure.resource_missing || failure.view_tree.is_some() {
        // One call: the read of the one close sibling fixes a mistyped name;
        // else the checkout that covers the path; else the parent's tree,
        // which shows what is there.
        let read = failure
            .corrected
            .and_then(|path| local_fetch_query::updated(q, serde_json::json!({"path":path})).ok())
            .map(FetchCall::related);
        let reclone = reclone.filter(|_| read.is_none());
        let next = NextCalls {
            view_tree: failure
                .view_tree
                .filter(|_| read.is_none() && reclone.is_none()),
            read,
            reclone,
            ..NextCalls::default()
        };
        result.next = Some(next).filter(|next| !next.is_empty());
    }
    result
}

#[cfg(test)]
mod tests;
