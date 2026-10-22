//! The bounded read of a local source file a tool parses or outlines: the
//! engine's regular-file read (non-blocking open, size from the opened
//! handle, read capped at the limit), so a FIFO, a device or a file grown
//! past the limit is never read whole.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use octocode_engine::stamp::SourceStamp;
pub(crate) use octocode_engine::text::ReadError;

/// Metadata identity for cache reuse. On Unix the change time and inode are
/// part of it, and a missing change time cannot prove that a write
/// preserving size and mtime left the source unchanged. Elsewhere the
/// platform has no change time: size and modification time are the proof,
/// so memos keep working there instead of never storing.
pub(crate) fn cache_stamp(meta: &std::fs::Metadata) -> Option<SourceStamp> {
    proof(SourceStamp::of(meta)?)
}

fn proof(stamp: SourceStamp) -> Option<SourceStamp> {
    (cfg!(not(unix)) || stamp.changed_ns != 0).then_some(stamp)
}

/// How a path is stat'ed: through a symlink, or the link itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Link {
    Follow,
    Itself,
}

/// `path`'s [`cache_stamp`]; with `settled_before`, only when it was last
/// written at least [`SourceStamp::SETTLE`] before then.
fn stamp_of(path: &Path, link: Link, settled_before: Option<SystemTime>) -> Option<SourceStamp> {
    let meta = match link {
        Link::Follow => std::fs::metadata(path),
        Link::Itself => std::fs::symlink_metadata(path),
    }
    .ok()?;
    proof(match settled_before {
        None => SourceStamp::of(&meta)?,
        Some(started) => SourceStamp::settled(&meta, started)?,
    })
}

/// The one "files unchanged" proof a memo keeps: each path's
/// [`cache_stamp`]. Shared, so a page checks it without copying the paths.
#[derive(Clone, Debug)]
pub(crate) struct StampSet {
    link: Link,
    stamps: Arc<[(PathBuf, SourceStamp)]>,
}

impl StampSet {
    /// The stamps of `paths`; `None` when one is gone, or, with
    /// `settled_before`, was written within [`SourceStamp::SETTLE`] before it
    /// (a write could race the reads the memo was built from).
    pub(crate) fn capture(
        paths: impl IntoIterator<Item = PathBuf>,
        link: Link,
        settled_before: Option<SystemTime>,
    ) -> Option<Self> {
        let stamps = paths
            .into_iter()
            .map(|path| stamp_of(&path, link, settled_before).map(|stamp| (path, stamp)))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            link,
            stamps: stamps.into(),
        })
    }

    /// Whether every path still has its stamp.
    pub(crate) fn unchanged(&self) -> bool {
        self.stamps
            .iter()
            .all(|(path, stamp)| stamp_of(path, self.link, None).as_ref() == Some(stamp))
    }
}

/// Bytes of the regular file at `path`, at most `max_bytes`.
pub(crate) fn read_bounded(path: &Path, max_bytes: usize) -> Result<Vec<u8>, ReadError> {
    octocode_engine::text::read_source(path, u64::try_from(max_bytes).unwrap_or(u64::MAX))
        .map(|source| source.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One proof on every platform: a stamp is captured (not refused for a
    /// missing change time), stands while nothing changes, and falls with an
    /// edit, a sibling's arrival in a stamped directory, or a deletion.
    #[test]
    fn a_stamp_set_proves_paths_unchanged_until_one_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.rs");
        std::fs::write(&file, "fn a() {}").expect("write");
        let paths = || [dir.path().to_path_buf(), file.clone()];
        let set = StampSet::capture(paths(), Link::Itself, None).expect("stamped");
        assert!(set.unchanged());
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join("b.rs"), "fn b() {}").expect("sibling");
        assert!(!set.unchanged(), "the directory changed");
        let set = StampSet::capture(paths(), Link::Follow, None).expect("stamped");
        std::fs::remove_file(&file).expect("remove");
        assert!(!set.unchanged(), "a deleted path is a change");
        assert!(StampSet::capture(paths(), Link::Follow, None).is_none());
    }

    #[test]
    fn a_stamp_set_needs_settled_paths_when_asked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("a.rs");
        std::fs::write(&file, "fn a() {}").expect("write");
        let now = SystemTime::now();
        assert!(StampSet::capture([file.clone()], Link::Follow, Some(now)).is_none());
        let later = now + SourceStamp::SETTLE + std::time::Duration::from_secs(1);
        assert!(StampSet::capture([file], Link::Follow, Some(later)).is_some());
    }

    #[test]
    fn a_file_past_the_limit_is_refused_before_it_is_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.rs");
        std::fs::write(&path, "x".repeat(65)).expect("write");
        assert!(matches!(
            read_bounded(&path, 64),
            Err(ReadError::TooLarge(65))
        ));
    }

    #[test]
    fn a_directory_is_no_source() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read_bounded(dir.path(), 1024).is_err());
    }
}
