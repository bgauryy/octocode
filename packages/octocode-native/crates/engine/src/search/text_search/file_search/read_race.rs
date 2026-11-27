//! Tests: writes made while a search reads a file, as a writer racing the
//! search makes them: after each read that returned bytes of the file, before
//! the search reads it again.

/// A write registered for a canonical source path, given the offset the
/// search's read of the file has reached. It returns whether it stays
/// registered for the next read: a writer that keeps rewriting the file
/// returns `true`, one write returns `false`.
pub type ReadRace = (std::path::PathBuf, Box<dyn FnMut(u64) -> bool + Send>);

/// The writes [`during_read`] makes, by canonical source path.
pub static DURING_SEARCH_READ: std::sync::Mutex<Vec<ReadRace>> = std::sync::Mutex::new(Vec::new());

/// Makes the write registered for `path`, if any, the read at `offset`.
pub(super) fn during_read(path: &std::path::Path, offset: u64) {
    let mut races = DURING_SEARCH_READ
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if races.is_empty() {
        return;
    }
    let Ok(source) = std::fs::canonicalize(path) else {
        return;
    };
    if let Some(at) = races.iter().position(|(path, _)| *path == source)
        && !(races[at].1)(offset)
    {
        drop(races.remove(at));
    }
}
