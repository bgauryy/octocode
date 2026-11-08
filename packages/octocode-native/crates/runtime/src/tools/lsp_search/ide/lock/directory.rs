//! The lock directory: judged by its own entry, its `*.json` files listed.
use octocode_engine::stamp::{SourceStamp, settle_clock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Why the lock directory cannot be trusted to hold locks: another user
/// could replace one.
#[cfg(unix)]
fn untrusted_directory(metadata: &std::fs::Metadata) -> Option<&'static str> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Some("is not owned by the current user");
    }
    (metadata.mode() & 0o022 != 0).then_some("is writable by its group or others")
}

#[cfg(not(unix))]
fn untrusted_directory(_: &std::fs::Metadata) -> Option<&'static str> {
    None
}

/// What the lock directory is.
pub(super) enum Directory {
    /// No such directory (or not a directory): no editor has written here.
    Absent,
    /// A directory (or a link to one) that cannot be trusted, with why.
    Refused(String),
    /// The directory's `*.json` files.
    Files(Vec<PathBuf>),
}

/// The directory, judged by its own entry (a symbolic link is refused,
/// never followed), and its `*.json` files memoized by its settled
/// stamp: a lock written or removed (the extension renames atomically)
/// changes it. Each file is still read on every call, so a mode change is
/// seen at once.
pub(super) fn directory(dir: &Path) -> Directory {
    /// Each directory's listing, with the stamp it was read at.
    type Listings = HashMap<PathBuf, (SourceStamp, Vec<PathBuf>)>;
    static MEMO: std::sync::LazyLock<Mutex<Listings>> = std::sync::LazyLock::new(Mutex::default);
    let Ok(entry) = std::fs::symlink_metadata(dir) else {
        return Directory::Absent;
    };
    let problem = if entry.file_type().is_symlink() {
        Some("is a symbolic link")
    } else if entry.is_dir() {
        untrusted_directory(&entry)
    } else {
        return Directory::Absent;
    };
    if let Some(problem) = problem {
        return Directory::Refused(format!(
            "The IDE bridge lock directory {problem}; it was not used."
        ));
    }
    // A listing is kept only for a directory untouched for the settle window:
    // a lock written within the clock tick of the read leaves the same time.
    let settled = SourceStamp::settled(&entry, settle_clock());
    let mut memo = MEMO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let (Some(stamp), Some((seen, files))) = (settled, memo.get(dir))
        && *seen == stamp
    {
        return Directory::Files(files.clone());
    }
    let mut files = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "json")
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    files.sort();
    match settled {
        Some(stamp) => memo.insert(dir.to_owned(), (stamp, files.clone())),
        None => memo.remove(dir),
    };
    Directory::Files(files)
}

