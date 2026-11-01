//! A listed file's line count: counted on bounded workers, revalidated
//! right before each read.
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
use octocode_contract_types::ErrorCode;

/// A file's line count as a listing shows it: counted, or why not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lines {
    Counted(usize),
    /// Binary content (the one binary definition: the leading-byte sniff, or
    /// a NUL anywhere) has no lines to count.
    Binary,
    /// The file could not be read (gone, denied, a read error): its count
    /// is unknown, not zero.
    Unreadable,
}

impl Lines {
    /// The entry field: `totalLines=N`, or the reason there is none.
    pub(super) fn field(self) -> String {
        match self {
            Self::Counted(lines) => format!("totalLines={lines}"),
            Self::Binary => "binary".to_owned(),
            Self::Unreadable => "unreadable".to_owned(),
        }
    }

    /// The field a details listing shows: none for a file without lines.
    pub(super) fn shown_in_details(self) -> Option<Self> {
        (self != Self::Counted(0)).then_some(self)
    }

    /// The line sort's key: counted files by count, most first, then the
    /// files without a count, then directories.
    pub(super) fn rank(lines: Option<Self>) -> (u8, std::cmp::Reverse<usize>) {
        match lines {
            Some(Self::Counted(lines)) => (0, std::cmp::Reverse(lines)),
            Some(Self::Binary) => (1, std::cmp::Reverse(0)),
            Some(Self::Unreadable) => (2, std::cmp::Reverse(0)),
            None => (3, std::cmp::Reverse(0)),
        }
    }
}

/// Line-count workers opening files at once. Each validates, opens and
/// reads files, and kernel time grows with concurrent openers while wall
/// time levels off: over 2,280 files (warm cache, 12 cores, macOS) 4 workers
/// took 38-42 ms wall and 0.13 s sys, 12 workers 54-57 ms and 0.55-0.62 s.
const FILE_OPEN_WORKERS: usize = 4;

/// Workers for counting `entries` files ([`crate::tools::workers::workers`]),
/// at most [`FILE_OPEN_WORKERS`].
fn line_count_workers(entries: usize) -> usize {
    crate::tools::workers::workers(entries, Some(FILE_OPEN_WORKERS))
}

/// The line counts of `files` (`None`: a directory, never counted), on
/// [`line_count_workers`]: each worker takes the next file until none is
/// left or `deadline` passes (the first file is always counted, so a cut
/// walk still advances). With the deadline passed first, also the index of
/// the first file left uncounted: the counts are a prefix of `files` up to
/// it.
pub(crate) fn count_lines(
    files: &[Option<&std::path::Path>],
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
    deadline: Option<std::time::Instant>,
) -> Result<(Vec<Option<Lines>>, Option<usize>), ToolError> {
    let workers = line_count_workers(files.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let first_file = files.iter().position(Option::is_some);
    let counted = std::thread::scope(|scope| {
        let tasks = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut counted = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(file) = files.get(index) else {
                            return Ok(counted);
                        };
                        let Some(path) = file else { continue };
                        let late = Some(index) != first_file
                            && deadline
                                .is_some_and(|at| octocode_engine::scan_budget::walk_now() >= at);
                        let lines = if late {
                            None
                        } else {
                            Some(line_count(path, paths, cancel)?)
                        };
                        counted.push((index, lines));
                    }
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| {
                task.join().unwrap_or_else(|_| {
                    Err(ToolError::new(
                        ErrorCode::ExecutionFailed,
                        "A line-count worker failed.",
                    ))
                })
            })
            .collect::<Result<Vec<_>, ToolError>>()
    })?;
    let mut lines = vec![None; files.len()];
    let mut cut = None::<usize>;
    for (index, count) in counted.into_iter().flatten() {
        match count {
            Some(count) => lines[index] = count,
            None => cut = Some(cut.map_or(index, |at| at.min(index))),
        }
    }
    if let Some(at) = cut {
        lines.truncate(at);
    }
    Ok((lines, cut))
}

/// The lines of the file at `path`, revalidated against the path policy
/// right before it is read. `None` for a listed link to a directory: like a
/// directory it has no lines, and is no failed read.
pub(super) fn line_count(
    path: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<Lines>, ToolError> {
    use std::io::Read;
    #[cfg(test)]
    LINE_COUNT_READS.with(|reads| reads.set(reads.get() + 1));
    cancel.check().map_err(ToolError::cancelled)?;
    // Revalidate immediately before reading: discovery authorization is not
    // permission to follow a changed link or open a special file: a link to
    // a directory has no lines, and `open_regular` refuses any other
    // non-regular target (a FIFO).
    let Ok(validated) = paths.validate(path) else {
        return Ok(Some(Lines::Unreadable));
    };
    if validated.canonical.is_dir() {
        return Ok(None);
    }
    let Ok((mut file, _)) = octocode_engine::text::open_regular(&validated.canonical) else {
        return Ok(Some(Lines::Unreadable));
    };
    let mut buffer = [0_u8; 16 * 1024];
    // Newlines terminate lines; a non-empty final line without one still counts.
    let mut lines = 0_usize;
    let mut last_byte = None;
    loop {
        cancel.check().map_err(ToolError::cancelled)?;
        match file.read(&mut buffer) {
            Ok(0) => {
                return Ok(Some(Lines::Counted(
                    lines + usize::from(last_byte.is_some_and(|b| b != b'\n')),
                )));
            }
            Ok(bytes) => {
                let read = &buffer[..bytes];
                if (last_byte.is_none()
                    && octocode_engine::text::sniff(read)
                        == octocode_engine::text::BinaryVerdict::Binary)
                    || read.contains(&0)
                {
                    return Ok(Some(Lines::Binary));
                }
                lines += read.iter().filter(|byte| **byte == b'\n').count();
                last_byte = read.last().copied();
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Ok(Some(Lines::Unreadable)),
        }
    }
}

#[cfg(test)]
thread_local! {
    static LINE_COUNT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn line_count_reads() -> usize {
    LINE_COUNT_READS.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests;
