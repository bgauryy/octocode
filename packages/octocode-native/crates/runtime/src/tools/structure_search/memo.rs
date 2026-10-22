//! Walked listings kept for continuation pages, keyed by snapshot and the
//! path policy that produced them, so page 2 onward does not walk the tree
//! again. A stored walk is served only while every listed entry and every
//! directory holding one keeps its size and modification time (an entry
//! created, removed or renamed there changes the directory's time); the
//! executor then recomputes the snapshot from the submitted query and the
//! stored rows before serving a page. The ignore rules the walk applied are
//! watched as astSearch's scan memo watches them ([`Watch`]): each
//! `.gitignore`/`.ignore` of the root and of every directory holding a
//! listed entry, the rule files outside the walk and the repository markers
//! above it, so an edited rule drops the walk even when no listed entry
//! changed. An entry created in a directory that held no listed entry is not
//! seen until the walk expires.
use super::super::page_memo::{
    PageMemo, Proof, Watch, directory_ignore_inputs, repository_markers,
};
use crate::tools::source::Link;
use std::{
    any::Any,
    sync::Arc,
    time::{Duration, SystemTime},
};

const MAX_ENTRIES: usize = 16;
/// Listed rows all stored walks may hold together; the oldest go first.
const MAX_TOTAL_ROWS: usize = 400_000;
const TTL: Duration = Duration::from_secs(60);

/// A listed entry's size and modification time as `symlink_metadata` reads
/// them now (a directory's size is 0).
fn observed(path: &std::path::Path) -> Option<(u64, Option<SystemTime>)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some((
        if meta.is_dir() { 0 } else { meta.len() },
        meta.modified().ok(),
    ))
}

/// A listed entry with the size and time its walk read (a directory's size
/// is 0, as [`observed`] reads it).
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
        let directory = entry.entry_type == octocode_engine::types::FileKind::Directory;
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

/// The proof of `listed` entries, every directory holding one, the walked
/// `root` and the ignore rules that shaped the walk (each directory's
/// [`directory_ignore_inputs`] under `paths`, and the rule files outside
/// the walk); `None` when a listed entry no longer has the size and time
/// its walk read (it changed before it could be stamped), or an ignore file
/// is one no stamp can watch. `inside` are directories the walk holds open
/// (a cursor's): their own ignore files are watched too, as a directory
/// holding a listed entry's are.
pub(super) fn proof(
    root: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    listed: Vec<Seen<'_>>,
    inside: &[&std::path::Path],
) -> Option<Proof> {
    let sources = listed
        .iter()
        .map(|seen| seen.path.to_path_buf())
        .collect::<Vec<_>>();
    let mut dirs = std::collections::BTreeSet::from([root.to_path_buf()]);
    dirs.extend(
        sources
            .iter()
            .filter_map(|source| source.parent())
            .filter(|parent| parent.starts_with(root))
            .map(std::path::Path::to_path_buf),
    );
    dirs.extend(inside.iter().map(|dir| dir.to_path_buf()));
    let mut rules = octocode_engine::search::walk::external_ignore_inputs(root);
    for dir in &dirs {
        let inputs =
            directory_ignore_inputs(dir, root, |ignore| paths.validate_read(ignore).is_ok());
        if inputs.untracked {
            return None;
        }
        rules.extend(inputs.files);
        rules.extend(inputs.external);
    }
    let proof = Watch {
        root,
        sources: &sources,
        rules: &rules,
        markers: &repository_markers(root),
    }
    .capture(Link::Itself, None)?;
    listed
        .iter()
        .all(|seen| observed(seen.path) == Some((seen.size, seen.modified)))
        .then_some(proof)
}

struct Walk {
    proof: Proof,
    walk: Arc<dyn Any + Send + Sync>,
}

/// Stored walks, weighed by listed rows.
static STORE: PageMemo<Walk> = PageMemo::new(TTL, MAX_ENTRIES, MAX_TOTAL_ROWS);

/// The stored walk for `snapshot` when its entries are unchanged; a changed
/// one is evicted.
pub(super) fn get<T: Any + Send + Sync>(snapshot: &str, policy: &str) -> Option<Arc<T>> {
    STORE
        .get_current(
            snapshot,
            policy,
            |stored| (Arc::clone(&stored.walk), stored.proof.clone()),
            Some,
        )?
        .downcast::<T>()
        .ok()
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
    paths: &crate::policy::path::PathPolicy,
    root: &std::path::Path,
    walk: &Arc<T>,
) {
    let listed = walk.sources();
    let rows = listed.len();
    if rows > MAX_TOTAL_ROWS {
        return;
    }
    let Some(proof) = proof(root, paths, listed, &[]) else {
        return;
    };
    let walk: Arc<dyn Any + Send + Sync> = Arc::clone(walk) as _;
    STORE.put(snapshot, policy, rows, Walk { proof, walk });
}

pub(super) fn evict(snapshot: &str) {
    STORE.evict(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::source::StampSet;
    use std::path::PathBuf;

    #[test]
    fn a_stored_walk_is_served_until_a_listed_directory_changes() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.txt");
        std::fs::write(&file, "x").expect("file");
        struct One(PathBuf);
        impl Listed for One {
            fn sources(&self) -> Vec<Seen<'_>> {
                let (size, modified) = observed(&self.0).expect("listed file");
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
            &crate::tools::test_support::workspace_policy(root.path()),
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

    /// An in-place edit of an ignore file beside the listed entries (a
    /// hidden one the listing never shows) drops the stored walk, as
    /// astSearch's scan memo does: the rules decide what the walk listed.
    #[test]
    fn a_stored_walk_is_dropped_when_an_ignore_rule_is_edited() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.txt");
        std::fs::write(&file, "x").expect("file");
        let rules = root.path().join(".gitignore");
        std::fs::write(&rules, "b.txt\n").expect("rules");
        struct One(PathBuf);
        impl Listed for One {
            fn sources(&self) -> Vec<Seen<'_>> {
                let (size, modified) = observed(&self.0).expect("listed file");
                vec![Seen {
                    path: self.0.as_path(),
                    size,
                    modified,
                }]
            }
        }
        let policy = "p".to_owned();
        put(
            "memo-rules".into(),
            policy.clone(),
            &crate::tools::test_support::workspace_policy(root.path()),
            root.path(),
            &Arc::new(One(file.clone())),
        );
        assert!(get::<One>("memo-rules", &policy).is_some());
        std::thread::sleep(Duration::from_millis(20));
        std::fs::OpenOptions::new()
            .append(true)
            .open(&rules)
            .and_then(|mut rules| std::io::Write::write_all(&mut rules, b"c.txt\n"))
            .expect("edit rules in place");
        assert!(get::<One>("memo-rules", &policy).is_none());
    }

    #[test]
    fn restored_mtime_does_not_keep_stale_listing_detail() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.txt");
        std::fs::write(&file, "ab\n").expect("initial source");
        let stamp = StampSet::capture([file.clone()], Link::Itself, None).expect("stamp");
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
        let seen = |size| Seen {
            path: &child,
            size,
            modified,
        };
        let paths = crate::tools::test_support::workspace_policy(root.path());
        let stamp =
            proof(root.path(), &paths, vec![seen(0)], &[]).expect("a directory stamps at size 0");
        assert!(stamp.holds(), "nothing changed since the walk");
        // Any other size is a change the walk did not read.
        assert!(proof(root.path(), &paths, vec![seen(1)], &[]).is_none());
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(child.join("new.txt"), "x").expect("newcomer");
        assert!(
            !stamp.holds(),
            "an entry created in it changes the directory"
        );
    }

    /// The ignore rules of a repository nested in the walk include its own
    /// exclude file, outside the walk: an edit there drops the stored walk.
    /// An ignore file no stamp can watch (a link) leaves the walk unstored.
    #[cfg(unix)]
    #[test]
    fn a_nested_repository_exclude_and_a_linked_rule_decide_the_proof() {
        let root = tempfile::tempdir().expect("root");
        let nested = root.path().join("nested");
        std::fs::create_dir_all(nested.join(".git").join("info")).expect("nested repository");
        let exclude = nested.join(".git").join("info").join("exclude");
        std::fs::write(&exclude, "b.txt\n").expect("exclude");
        let file = nested.join("a.txt");
        std::fs::write(&file, "x").expect("file");
        let paths = crate::tools::test_support::workspace_policy(root.path());
        let listed = || {
            let (size, modified) = observed(&file).expect("listed file");
            vec![Seen {
                path: file.as_path(),
                size,
                modified,
            }]
        };
        let stamp = proof(root.path(), &paths, listed(), &[]).expect("stamped");
        assert!(stamp.holds());
        std::thread::sleep(Duration::from_millis(20));
        std::fs::OpenOptions::new()
            .append(true)
            .open(&exclude)
            .and_then(|mut rules| std::io::Write::write_all(&mut rules, b"c.txt\n"))
            .expect("edit the nested exclude in place");
        assert!(!stamp.holds(), "the nested repository's rules changed");
        std::os::unix::fs::symlink(&exclude, nested.join(".gitignore")).expect("link");
        assert!(proof(root.path(), &paths, listed(), &[]).is_none());
    }
}
