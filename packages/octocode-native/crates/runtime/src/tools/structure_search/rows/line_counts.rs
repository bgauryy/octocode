//! A listed file's line count: counted on bounded workers, revalidated
//! right before each read.
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;

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

    /// Where the line sort places an entry in either direction: counted
    /// files, then the files without a count, then directories.
    pub(super) fn class(lines: Option<Self>) -> u8 {
        match lines {
            Some(Self::Counted(_)) => 0,
            Some(Self::Binary) => 1,
            Some(Self::Unreadable) => 2,
            None => 3,
        }
    }

    /// The line sort's key among counted files.
    pub(super) fn count(lines: Option<Self>) -> usize {
        match lines {
            Some(Self::Counted(lines)) => lines,
            _ => 0,
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
    let first_file = files.iter().position(Option::is_some);
    let mut indexes = (0..files.len()).collect::<Vec<_>>();
    // Per file: `None` once the deadline passed, else its count (`None`
    // for an entry that is not a file).
    let counted = crate::tools::workers::try_each_parallel(
        &mut indexes,
        line_count_workers(files.len()),
        |&mut index| -> Result<Option<Option<Lines>>, ToolError> {
            let Some(path) = files[index] else {
                return Ok(Some(None));
            };
            let late = Some(index) != first_file
                && deadline.is_some_and(|at| octocode_engine::scan_budget::walk_now() >= at);
            if late {
                return Ok(None);
            }
            line_count(path, paths, cancel).map(Some)
        },
    )?;
    let mut lines = vec![None; files.len()];
    let mut cut = None::<usize>;
    for (index, count) in counted.into_iter().enumerate() {
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

/// Lines of a text with a UTF-16 byte order mark, by code unit: a byte 0x0A or
/// 0x00 alone says nothing there.
struct Utf16Lines {
    big_endian: bool,
    /// The first byte of a code unit a read split.
    carried: Option<u8>,
    /// The mark, the first unit, is no text.
    units: usize,
    lines: usize,
    last_unit: u16,
}

impl Utf16Lines {
    fn of(first: &[u8]) -> Option<Self> {
        let big_endian = match first {
            [0xff, 0xfe, ..] => false,
            [0xfe, 0xff, ..] => true,
            _ => return None,
        };
        Some(Self {
            big_endian,
            carried: None,
            units: 0,
            lines: 0,
            last_unit: 0,
        })
    }

    /// Reads `bytes`; `false` on a NUL code unit, which makes the text binary as
    /// a read of it does.
    fn feed(&mut self, bytes: &[u8]) -> bool {
        let mut rest = bytes;
        if let (Some(first), Some((second, tail))) = (self.carried, rest.split_first()) {
            self.carried = None;
            rest = tail;
            if !self.unit([first, *second]) {
                return false;
            }
        }
        let (pairs, odd) = rest.as_chunks::<2>();
        for pair in pairs {
            if !self.unit(*pair) {
                return false;
            }
        }
        if let [odd] = odd {
            self.carried = Some(*odd);
        }
        true
    }

    fn unit(&mut self, pair: [u8; 2]) -> bool {
        let unit = if self.big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        };
        self.units += 1;
        if self.units == 1 {
            return true;
        }
        self.lines += usize::from(unit == 0x0a);
        self.last_unit = unit;
        unit != 0
    }

    fn finish(self) -> Lines {
        if self.carried.is_some() {
            return Lines::Binary;
        }
        Lines::Counted(self.lines + usize::from(self.units > 1 && self.last_unit != 0x0a))
    }
}

/// The lines of the file at `path`, revalidated against the path policy
/// right before it is read. `None` for a listed link to a directory: like a
/// directory it has no lines, and is no failed read.
fn line_count(
    path: &std::path::Path,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<Option<Lines>, ToolError> {
    use std::io::Read;
    #[cfg(test)]
    record_line_count_read(path);
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
    let mut lines = octocode_engine::text::LineCount::default();
    let mut first = true;
    let mut wide = None::<Utf16Lines>;
    loop {
        cancel.check().map_err(ToolError::cancelled)?;
        match file.read(&mut buffer) {
            Ok(0) => {
                if let Some(wide) = wide {
                    return Ok(Some(wide.finish()));
                }
                return Ok(Some(Lines::Counted(lines.lines())));
            }
            Ok(bytes) => {
                let read = &buffer[..bytes];
                if std::mem::take(&mut first) {
                    if octocode_engine::text::sniff(read)
                        == octocode_engine::text::BinaryVerdict::Binary
                    {
                        return Ok(Some(Lines::Binary));
                    }
                    wide = Utf16Lines::of(read);
                }
                if let Some(wide) = wide.as_mut() {
                    if !wide.feed(read) {
                        return Ok(Some(Lines::Binary));
                    }
                } else if read.contains(&0) {
                    return Ok(Some(Lines::Binary));
                } else {
                    lines.feed(read);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Ok(Some(Lines::Unreadable)),
        }
    }
}

/// Every file a line count read, by canonical path, so a test counts the
/// reads under its own fixture whichever thread ran them: a count runs on
/// worker threads or inline on the caller ([`crate::tools::workers`]), as
/// the process CPU share of the moment decides.
#[cfg(test)]
static LINE_COUNT_READS: std::sync::Mutex<Vec<std::path::PathBuf>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn record_line_count_read(path: &std::path::Path) {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    LINE_COUNT_READS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(path);
}

/// The line-count reads so far of files under `root`.
#[cfg(test)]
pub(crate) fn line_count_reads(root: &std::path::Path) -> usize {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    LINE_COUNT_READS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|path| path.starts_with(&root))
        .count()
}

#[cfg(test)]
mod tests;
