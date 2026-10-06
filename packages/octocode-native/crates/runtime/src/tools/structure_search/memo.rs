//! Walked listings kept for continuation pages, keyed by snapshot and the
//! path policy that produced them, so page 2 onward does not walk the tree
//! again. A stored walk is served only while every listed entry and every
//! directory holding one keeps its size and modification time (an entry
//! created, removed or renamed there changes the directory's time); the
//! executor then recomputes the snapshot from the submitted query and the
//! stored rows before serving a page. An entry created in a directory that
//! held no listed entry is not seen until the walk expires.
use std::{
    any::Any,
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

const MAX_ENTRIES: usize = 16;
/// Listed rows all stored walks may hold together; the oldest go first.
const MAX_TOTAL_ROWS: usize = 400_000;
const TTL: Duration = Duration::from_secs(60);

/// An entry as it was when its walk was stored.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Stamp {
    path: PathBuf,
    size: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    /// The entry's current size and time, `None` when it is gone.
    pub(super) fn of(path: PathBuf) -> Option<Self> {
        let meta = std::fs::symlink_metadata(&path).ok()?;
        Some(Self {
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta.modified().ok(),
            path,
        })
    }

    fn unchanged(&self) -> bool {
        Self::of(self.path.clone()).as_ref() == Some(self)
    }
}

/// The stamps of `listed` entries plus every directory holding one and the
/// walked `root`, deduplicated.
fn stamps<'a>(
    root: &std::path::Path,
    listed: impl Iterator<Item = &'a std::path::Path>,
) -> Vec<Stamp> {
    let mut paths = std::collections::BTreeSet::new();
    paths.insert(root.to_path_buf());
    for path in listed {
        paths.insert(path.to_path_buf());
        if let Some(parent) = path.parent() {
            paths.insert(parent.to_path_buf());
        }
    }
    paths.into_iter().filter_map(Stamp::of).collect()
}

struct Entry {
    snapshot: String,
    policy: String,
    stored: Instant,
    rows: usize,
    stamps: Vec<Stamp>,
    walk: Arc<dyn Any + Send + Sync>,
}

static STORE: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

/// The stored walk for `snapshot` when its entries are unchanged; a changed
/// one is evicted.
pub(super) fn get<T: Any + Send + Sync>(snapshot: &str, policy: &str) -> Option<Arc<T>> {
    let (walk, stamps) = {
        let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
        store.retain(|entry| entry.stored.elapsed() < TTL);
        let entry = store
            .iter()
            .find(|entry| entry.snapshot == snapshot && entry.policy == policy)?;
        (Arc::clone(&entry.walk), entry.stamps.clone())
    };
    if stamps.iter().all(Stamp::unchanged) {
        return walk.downcast::<T>().ok();
    }
    evict(snapshot);
    None
}

/// A stored walk: the entries it listed, whose stamps keep it current.
pub(super) trait Listed: Any + Send + Sync {
    fn sources(&self) -> Vec<&std::path::Path>;
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
    let stamps = stamps(root, listed.into_iter());
    let walk = Arc::clone(walk);
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.retain(|entry| entry.snapshot != snapshot && entry.stored.elapsed() < TTL);
    store.push_back(Entry {
        snapshot,
        policy,
        stored: Instant::now(),
        rows,
        stamps,
        walk,
    });
    while store.len() > MAX_ENTRIES
        || store.iter().map(|entry| entry.rows).sum::<usize>() > MAX_TOTAL_ROWS
    {
        store.pop_front();
    }
}

pub(super) fn evict(snapshot: &str) {
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.retain(|entry| entry.snapshot != snapshot);
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
            fn sources(&self) -> Vec<&std::path::Path> {
                vec![self.0.as_path()]
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
}
