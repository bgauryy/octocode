//! Complete UTF-8 configuration reads, bounded by the opened regular file.
use std::io;
use std::path::Path;

/// Read a whole configuration without imposing a separate configuration-size
/// limit. Resolve intentional symlinks, then open the target without blocking
/// on special files. Its observed length bounds the read: growth is an error,
/// never a parsed prefix. Allocation failure and cancellation remain errors.
pub fn read_utf8(path: &Path) -> io::Result<String> {
    check_cancelled()?;
    let path = std::fs::canonicalize(path)?;
    let (file, metadata) = crate::text::open_regular(&path)?;
    read_opened(file, metadata.len())
}

fn read_opened(file: std::fs::File, length: u64) -> io::Result<String> {
    check_cancelled()?;
    let capacity = usize::try_from(length).map_err(io::Error::other)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(io::Error::other)?;
    let bytes =
        crate::text::read_source_from(file, bytes, length, check_cancelled).map_err(|error| {
            match error {
                crate::text::ReadError::Io(error) => error,
                error => io::Error::new(io::ErrorKind::InvalidData, error.to_string()),
            }
        })?;
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn check_cancelled() -> io::Result<()> {
    if crate::deep_stack::caller_stopped() || crate::deep_stack::job_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "configuration read cancelled",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
