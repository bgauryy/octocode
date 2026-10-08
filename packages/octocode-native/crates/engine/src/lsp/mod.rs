pub mod client;
mod commands;
pub mod config;
pub(crate) mod grammar;
pub mod managed;
pub mod pool;
mod process_tree;
pub mod resolver;
mod spawn_limits;
pub(crate) mod transport;
pub mod types;
pub mod uri;
pub(crate) mod validation;
pub mod workspace;

/// Worst configured cold-start path before process-start and transport
/// overhead: initialize + readiness + one request with all content-modified
/// retries and their delays. Host deadlines must be strictly larger.
pub const MAX_COLD_LSP_EXECUTION_BUDGET_MS: u64 = (client::REQUEST_TIMEOUT_MS as u64)
    * (2 + client::CONTENT_MODIFIED_RETRIES as u64)
    + pool::MAX_READINESS_TIMEOUT_MS
    + client::CONTENT_MODIFIED_RETRY_DELAY_MS * client::CONTENT_MODIFIED_RETRIES as u64;

/// Largest source the LSP layer reads, synchronizes (`didOpen`), resolves
/// anchors in, or cuts snippets from. One bound for every path so they cannot
/// drift; sized for real monolithic sources (TypeScript's 3 MB checker.ts).
pub const MAX_LSP_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

/// Why [`read_regular_bounded`] refused a path.
#[derive(Debug)]
pub enum BoundedRead {
    /// The length `fstat` or the capped read found, above the limit.
    TooLarge(u64),
    /// A FIFO, a device, a directory, or another non-regular file.
    NotRegular,
    Io(std::io::Error),
}

/// Bytes of the regular file at `path`, at most `max_bytes`. The open never
/// blocks (`O_NONBLOCK` on unix, so a FIFO cannot hang it); the opened handle
/// is checked with `fstat` and read through `take(max_bytes + 1)`, so a file
/// that grew past the limit is rejected without reading it whole.
pub fn read_regular_bounded(
    path: &std::path::Path,
    max_bytes: u64,
) -> std::result::Result<Vec<u8>, BoundedRead> {
    use std::io::Read;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(BoundedRead::Io)?;
    let metadata = file.metadata().map_err(BoundedRead::Io)?;
    if !metadata.is_file() {
        return Err(BoundedRead::NotRegular);
    }
    if metadata.len() > max_bytes {
        return Err(BoundedRead::TooLarge(metadata.len()));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(BoundedRead::Io)?;
    let read = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read > max_bytes {
        return Err(BoundedRead::TooLarge(read));
    }
    Ok(bytes)
}
