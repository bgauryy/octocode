//! Walked listings kept for continuation pages, keyed by snapshot and the
//! path policy that produced them, so page 2 onward does not walk the tree
//! again. A stored walk is served only while every directory it entered and
//! every listed entry whose metadata its rows show keeps its stamp (an entry
//! created, removed or renamed in a directory changes the directory's); the
//! executor then recomputes the snapshot from the submitted query and the
//! stored rows before serving a page. Those stamps are the ones the walk
//! read, taken before what they prove, so keeping a walk stats none of them.
//! The ignore rules the walk applied are watched as astSearch's scan memo
//! watches them ([`Watch`]): each `.gitignore`/`.ignore` of a directory it
//! entered, the rule files outside the walk and the repository markers above
//! it, so an edited rule drops the walk even when no listed entry changed.
use super::super::page_memo::{
    PageMemo, Proven, Watch, directory_ignore_inputs, repository_markers,
};
use octocode_engine::stamp::{Link, Proof};
use std::{sync::Arc, time::SystemTime};

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

/// The proof of a walk of `root`: the stamps it read of the directories it
/// `walked` and of its `listed` rows, the ignore files those directories'
/// listings showed and the rule files outside the walk. `None` when an ignore
/// file is one no stamp can watch: a link or other non-file, or one the
/// policy withholds. A directory the walk took from a resume cursor has no
/// stamp of this walk's and is stamped now, like a root that is a file.
pub(super) fn walk_proof(
    root: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    walked: &[octocode_engine::types::WalkedDir],
    listed: Vec<(std::path::PathBuf, octocode_engine::stamp::SourceStamp)>,
) -> Option<Proof> {
    let mut rules = octocode_engine::search::walk::external_ignore_inputs(root);
    let mut unstamped = Vec::new();
    let mut taken = listed;
    if !walked.iter().any(|dir| dir.path == root) {
        unstamped.push(root.to_path_buf());
    }
    for dir in walked {
        match dir.stamp {
            Some(stamp) => taken.push((dir.path.clone(), stamp)),
            None => unstamped.push(dir.path.clone()),
        }
        for (file, regular) in &dir.ignore_files {
            if !regular || paths.validate_read(file).is_err() {
                return None;
            }
            rules.push(file.clone());
        }
        if dir.holds_git && dir.path != root {
            rules.extend(octocode_engine::search::walk::external_ignore_inputs(
                &dir.path,
            ));
        }
    }
    Some(
        Watch {
            root,
            sources: &unstamped,
            rules: &rules,
            markers: &repository_markers(root),
        }
        .capture(Link::Itself, None)?
        .with_taken(Link::Itself, taken),
    )
}

/// Stored walks, weighed by the heap they and their proofs hold against the
/// one process budget: a walk too large for all of it is not kept.
static STORE: PageMemo<Proven<super::walked::Walked>> = PageMemo::new();

/// The stored walk for `snapshot` when its entries are unchanged; a changed
/// one is evicted.
pub(super) fn get(snapshot: &str, policy: &str) -> Option<Arc<super::walked::Walked>> {
    STORE.current(snapshot, policy)
}

/// Keep `walk` of `root` for the pages after the first.
pub(super) fn put(
    snapshot: String,
    policy: String,
    paths: &crate::policy::path::PathPolicy,
    root: &std::path::Path,
    walk: &Arc<super::walked::Walked>,
) {
    let Some(proof) = walk.proof(root, paths) else {
        return;
    };
    let bytes = walk.weight() + proof.weight();
    STORE.put_proven(snapshot, policy, bytes, proof, Arc::clone(walk));
}

pub(super) fn evict(snapshot: &str) {
    STORE.evict(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    /// Walks of one listed file, stored as `put` stores a walk.
    static ONES: PageMemo<Proven<PathBuf>> = PageMemo::new();

    fn put_one(snapshot: &str, policy: &str, root: &std::path::Path, file: &std::path::Path) {
        let (size, modified) = observed(file).expect("listed file");
        let listed = vec![Seen {
            path: file,
            size,
            modified,
        }];
        let paths = crate::tools::test_support::workspace_policy(root);
        let proof = proof(root, &paths, listed, &[]).expect("proof");
        ONES.put_proven(
            snapshot.to_owned(),
            policy.to_owned(),
            file.as_os_str().len(),
            proof,
            Arc::new(file.to_path_buf()),
        );
    }

    #[test]
    fn a_stored_walk_is_served_until_a_listed_directory_changes() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.txt");
        std::fs::write(&file, "x").expect("file");
        let policy = "p".to_owned();
        put_one("memo-test", &policy, root.path(), &file);
        let stored = ONES.current("memo-test", &policy);
        assert_eq!(stored.as_deref(), Some(&file));
        assert!(ONES.current("memo-test", "other policy").is_none());
        // A sibling created after the walk changes the directory's time.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(root.path().join("b.txt"), "y").expect("newcomer");
        assert!(ONES.current("memo-test", &policy).is_none());
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
        let policy = "p".to_owned();
        put_one("memo-rules", &policy, root.path(), &file);
        assert!(ONES.current("memo-rules", &policy).is_some());
        std::thread::sleep(Duration::from_millis(20));
        std::fs::OpenOptions::new()
            .append(true)
            .open(&rules)
            .and_then(|mut rules| std::io::Write::write_all(&mut rules, b"c.txt\n"))
            .expect("edit rules in place");
        assert!(ONES.current("memo-rules", &policy).is_none());
    }

    #[test]
    fn restored_mtime_does_not_keep_stale_listing_detail() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.txt");
        std::fs::write(&file, "ab\n").expect("initial source");
        let stamp = Proof::capture([file.clone()], Link::Itself, None).expect("stamp");
        let old_time = std::fs::metadata(&file)
            .expect("metadata")
            .modified()
            .expect("mtime");
        std::fs::write(&file, "a\nb").expect("same-size edit");
        std::fs::File::open(&file)
            .expect("file")
            .set_times(std::fs::FileTimes::new().set_modified(old_time))
            .expect("restore mtime");
        assert!(!stamp.holds(), "content changed with size and mtime intact");
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
