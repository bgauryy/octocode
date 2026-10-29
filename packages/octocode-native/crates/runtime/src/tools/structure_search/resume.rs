//! Where a listing walk cut by time stands, kept for the window that resumes
//! it (`next.continueWalk`, `walkOffset`): the directories the walk is
//! inside, with the entries still to inspect, so the resumed walk starts at
//! its first new entry and replays nothing (replaying a deep offset costs
//! most of a window's budget).
//!
//! A kept cursor is a candidate, never trusted alone: it serves only while
//! the engine accepts it (the same walk shape, exactly at the resume
//! position) and every directory it holds keeps the modification time its
//! listing was read at, and the ignore rules that shaped the walk are
//! unchanged ([`memo::proof`]). An entry created, removed or renamed in a
//! held directory, an edited rule, an expired or evicted cursor, or another
//! process (nothing was kept) all fall back to the exact replay, which lists
//! every entry once as it did before.
use super::super::page_memo::{PageMemo, Proof};
use super::memo::{self, Seen};
use octocode_engine::search::fs_resume::WalkCursor;
use serde_json::{Value, json};
use std::time::Duration;

const MAX_CURSORS: usize = 16;
/// Entries all kept cursors hold to inspect together, the oldest dropped
/// first.
const MAX_WEIGHT: usize = 200_000;
const TTL: Duration = Duration::from_secs(600);

struct Kept {
    proof: Proof,
    cursor: WalkCursor,
}

static STORE: PageMemo<Kept> = PageMemo::new(TTL, MAX_CURSORS, MAX_WEIGHT);

/// What the cursor belongs to: the walk (`identity`: the root and everything
/// that decides its order and what it enters) and the position it resumes at.
fn key(identity: &Value, position: u32) -> String {
    crate::digest::json_sha256(&json!(["structureSearch.walkCursor", identity, position]))
}

/// Keep `cursor`, where a walk of `root` stands, for the walk that resumes at
/// its position. Nothing is kept when a directory it holds cannot be stamped.
pub(super) fn keep(
    identity: &Value,
    policy: &str,
    paths: &crate::policy::path::PathPolicy,
    root: &std::path::Path,
    cursor: WalkCursor,
) {
    let directories = cursor.listed_directories();
    let held = directories.iter().map(|(dir, _)| *dir).collect::<Vec<_>>();
    let listed = directories
        .iter()
        .map(|(path, modified)| {
            modified.map(|modified| Seen {
                path,
                size: 0,
                modified: Some(modified),
            })
        })
        .collect::<Option<Vec<_>>>();
    let Some(proof) = listed.and_then(|listed| memo::proof(root, paths, listed, &held)) else {
        return;
    };
    let (key, weight) = (key(identity, cursor.position()), cursor.weight());
    STORE.put(key, policy.to_owned(), weight, Kept { proof, cursor });
}

/// The cursor kept for the walk that resumes at `position`, while nothing it
/// depends on changed; one that did is dropped.
pub(super) fn take(identity: &Value, policy: &str, position: u32) -> Option<WalkCursor> {
    let key = key(identity, position);
    let (proof, cursor) = STORE.get(&key, policy, |kept| {
        (kept.proof.clone(), kept.cursor.clone())
    })?;
    if proof.holds() {
        return Some(cursor);
    }
    STORE.evict(&key);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use octocode_engine::types::FileSystemQueryOptions;

    /// The cursor of a walk of `root` cut by time just after its first entry.
    fn cut_after_first_entry(root: &std::path::Path) -> WalkCursor {
        let result = octocode_engine::search::fs_query::query_file_system(
            FileSystemQueryOptions {
                path: root.to_string_lossy().into_owned(),
                recursive: Some(true),
                skip_entries: Some(1),
                time_budget: Some(Duration::ZERO),
                ..Default::default()
            },
            &|_, _, _| true,
            &|| false,
        )
        .expect("walk");
        result.cursor.expect("a walk cut by time keeps its cursor")
    }

    fn walk() -> (tempfile::TempDir, crate::policy::path::PathPolicy) {
        let root = tempfile::tempdir().expect("root");
        for file in ["a/1.txt", "a/2.txt", "b/3.txt"] {
            let path = root.path().join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, "x").expect("file");
        }
        let paths = crate::tools::test_support::workspace_policy(root.path());
        (root, paths)
    }

    #[test]
    fn a_kept_cursor_serves_its_walk_at_its_position_until_a_held_directory_changes() {
        let (root, paths) = walk();
        let identity = json!(["walk"]);
        keep(
            &identity,
            "p",
            &paths,
            root.path(),
            cut_after_first_entry(root.path()),
        );
        assert!(take(&identity, "other policy", 1).is_none());
        assert!(take(&json!(["another walk"]), "p", 1).is_none());
        assert!(take(&identity, "p", 2).is_none(), "another position");
        assert_eq!(
            take(&identity, "p", 1).map(|cursor| cursor.position()),
            Some(1)
        );
        // An entry created in a directory the cursor holds moves its time on.
        std::thread::sleep(Duration::from_millis(30));
        std::fs::write(root.path().join("a").join("0.txt"), "y").expect("newcomer");
        assert!(take(&identity, "p", 1).is_none(), "the listing is stale");
        assert!(take(&identity, "p", 1).is_none(), "and dropped");
    }

    #[test]
    fn an_edited_ignore_rule_of_the_directory_the_walk_is_inside_drops_the_cursor() {
        let (root, paths) = walk();
        let rules = root.path().join("a").join(".gitignore");
        std::fs::write(&rules, "2.txt\n").expect("rules");
        let identity = json!(["rules"]);
        keep(
            &identity,
            "p",
            &paths,
            root.path(),
            cut_after_first_entry(root.path()),
        );
        assert!(take(&identity, "p", 1).is_some());
        std::thread::sleep(Duration::from_millis(30));
        std::fs::OpenOptions::new()
            .append(true)
            .open(&rules)
            .and_then(|mut rules| std::io::Write::write_all(&mut rules, b"3.txt\n"))
            .expect("edit in place");
        assert!(take(&identity, "p", 1).is_none());
    }
}
