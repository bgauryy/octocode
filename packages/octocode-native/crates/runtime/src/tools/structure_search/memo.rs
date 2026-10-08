//! Walked listings kept for continuation pages, keyed by snapshot and the
//! path policy that produced them, so page 2 onward does not walk the tree
//! again. A stored walk is served only while every listed entry and every
//! directory holding one keeps its size and modification time (an entry
//! created, removed or renamed there changes the directory's time); the
//! executor then recomputes the snapshot from the submitted query and the
//! stored rows before serving a page. An entry created in a directory that
//! held no listed entry is not seen until the walk expires.
use super::super::page_memo::PageMemo;
use std::{
    any::Any,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

const MAX_ENTRIES: usize = 16;
/// Listed rows all stored walks may hold together; the oldest go first.
const MAX_TOTAL_ROWS: usize = 400_000;
const TTL: Duration = Duration::from_secs(60);

/// An entry as it was when its walk was stored.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Stamp {
    path: PathBuf,
    identity: octocode_engine::graph::SourceStamp,
}

impl Stamp {
    /// The entry's current size and time, `None` when it is gone.
    pub(super) fn of(path: PathBuf) -> Option<Self> {
        let meta = std::fs::symlink_metadata(&path).ok()?;
        let identity = crate::tools::source::cache_stamp(&meta)?;
        Some(Self { identity, path })
    }

    #[cfg(test)]
    fn read(path: &std::path::Path) -> Option<(u64, Option<SystemTime>)> {
        let meta = std::fs::symlink_metadata(path).ok()?;
        Some((
            if meta.is_dir() { 0 } else { meta.len() },
            meta.modified().ok(),
        ))
    }

    /// Whether the entry still has its stored size and time (no path copy).
    fn unchanged(&self) -> bool {
        std::fs::symlink_metadata(&self.path)
            .ok()
            .and_then(|meta| crate::tools::source::cache_stamp(&meta))
            == Some(self.identity)
    }
}

/// A listed entry with the size and time its walk read (a directory's size
/// is 0, as [`Stamp`] records it).
pub(super) struct Seen<'a> {
    pub path: &'a std::path::Path,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl<'a> Seen<'a> {
    /// A walked entry: its `symlink_metadata` size (0 for a directory) and
    /// exact modification time.
    pub(super) fn walked(
        path: &'a std::path::Path,
        entry: &octocode_engine::types::FileSystemEntry,
    ) -> Self {
        let directory = entry.entry_type == "directory";
        Self {
            path,
            size: if directory {
                0
            } else {
                u64::try_from(entry.size.unwrap_or(0)).unwrap_or(0)
            },
            modified: entry.modified_time,
        }
    }
}

/// The stamps of `listed` entries (as their walk read them, so no entry is
/// stat'ed again) plus every directory holding one and the walked `root`
/// (stat'ed now), deduplicated.
fn stamps(root: &std::path::Path, listed: Vec<Seen<'_>>) -> Option<Vec<Stamp>> {
    let mut known =
        std::collections::BTreeMap::<&std::path::Path, Option<(u64, Option<SystemTime>)>>::new();
    known.insert(root, None);
    for seen in &listed {
        if let Some(parent) = seen.path.parent() {
            known.entry(parent).or_insert(None);
        }
    }
    for seen in listed {
        known.insert(seen.path, Some((seen.size, seen.modified)));
    }
    known
        .into_iter()
        .map(|(path, seen)| {
            let stamp = Stamp::of(path.to_path_buf())?;
            if let Some((size, modified)) = seen
                && {
                    let meta = std::fs::symlink_metadata(path).ok()?;
                    let walked_size = if meta.is_dir() { 0 } else { meta.len() };
                    walked_size != size || meta.modified().ok() != modified
                }
            {
                return None;
            }
            Some(stamp)
        })
        .collect()
}

struct Walk {
    /// Shared so a page checks them without copying every stored path.
    stamps: Arc<[Stamp]>,
    walk: Arc<dyn Any + Send + Sync>,
}

/// Stored walks, weighed by listed rows.
static STORE: PageMemo<Walk> = PageMemo::new(TTL, MAX_ENTRIES, MAX_TOTAL_ROWS);

/// The stored walk for `snapshot` when its entries are unchanged; a changed
/// one is evicted.
pub(super) fn get<T: Any + Send + Sync>(snapshot: &str, policy: &str) -> Option<Arc<T>> {
    let (walk, stamps) = STORE.get(snapshot, policy, |stored| {
        (Arc::clone(&stored.walk), Arc::clone(&stored.stamps))
    })?;
    if stamps.iter().all(Stamp::unchanged) {
        return walk.downcast::<T>().ok();
    }
    evict(snapshot);
    None
}

/// A stored walk: the entries it listed, whose stamps keep it current.
pub(super) trait Listed: Any + Send + Sync {
    fn sources(&self) -> Vec<Seen<'_>>;
}

/// Keep `walk` of `root` for the pages after the first, stamped by its
/// listed entries.
pub(super) fn put<T: Listed>(
    snapshot: String,
    policy: String,
    root: &std::path::Path,
    walk: &Arc<T>,
) {
    let listed = walk.sources();
    let rows = listed.len();
    if rows > MAX_TOTAL_ROWS {
        return;
    }
    let Some(stamps) = stamps(root, listed) else {
        return;
    };
    let stamps = stamps.into();
    let walk: Arc<dyn Any + Send + Sync> = Arc::clone(walk) as _;
    STORE.put(snapshot, policy, rows, Walk { stamps, walk });
}

pub(super) fn evict(snapshot: &str) {
    STORE.evict(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_walk_is_served_until_a_listed_directory_changes() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.txt");
        std::fs::write(&file, "x").expect("file");
        struct One(PathBuf);
        impl Listed for One {
            fn sources(&self) -> Vec<Seen<'_>> {
                let (size, modified) = Stamp::read(&self.0).expect("listed file");
                vec![Seen {
                    path: self.0.as_path(),
                    size,
                    modified,
                }]
            }
        }
        let policy = "p".to_owned();
        put(
            "memo-test".into(),
            policy.clone(),
            root.path(),
            &Arc::new(One(file.clone())),
        );
        let stored = get::<One>("memo-test", &policy);
        assert_eq!(stored.as_deref().map(|one| &one.0), Some(&file));
        assert!(get::<One>("memo-test", "other policy").is_none());
        // A sibling created after the walk changes the directory's time.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.path().join("b.txt"), "y").expect("newcomer");
        assert!(get::<One>("memo-test", &policy).is_none());
    }

    #[test]
    fn restored_mtime_does_not_keep_stale_listing_detail() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.txt");
        std::fs::write(&file, "ab\n").expect("initial source");
        let stamp = Stamp::of(file.clone()).expect("stamp");
        let old_time = std::fs::metadata(&file)
            .expect("metadata")
            .modified()
            .expect("mtime");
        std::fs::write(&file, "a\nb").expect("same-size edit");
        std::fs::File::open(&file)
            .expect("file")
            .set_times(std::fs::FileTimes::new().set_modified(old_time))
            .expect("restore mtime");
        assert!(
            !stamp.unchanged(),
            "content changed with size and mtime intact"
        );
    }

    #[test]
    fn directory_row_uses_the_walks_zero_size_convention() {
        let root = tempfile::tempdir().expect("root");
        let child = root.path().join("empty");
        std::fs::create_dir(&child).expect("directory");
        let modified = std::fs::symlink_metadata(&child)
            .expect("metadata")
            .modified()
            .ok();
        assert!(
            stamps(
                root.path(),
                vec![Seen {
                    path: &child,
                    size: 0,
                    modified
                }]
            )
            .is_some()
        );
    }
}
