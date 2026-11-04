use super::*;
use crate::tools::lsp_search::words::file_spells;
use octocode_engine::stamp::{settle_clock, settle_written_so_far};
use std::collections::BTreeSet;
use std::path::PathBuf;

mod differential;
mod scope_requests;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    (dir, root)
}

fn spells(name: &str, path: &Path) -> Option<bool> {
    WordSearch::new(name, None, settle_clock()).file_spells(path)
}

fn kept_tokens(path: &Path) -> Option<usize> {
    let key = path.to_str().expect("utf-8 path");
    SPELLED.get(key, ANY_POLICY, |kept| kept.tokens.len())
}

/// A settled file's entry answers every absent name; only a name it may hold
/// opens the file.
#[test]
fn a_settled_file_is_opened_only_for_a_name_it_may_spell() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    std::fs::write(&file, "export function present() {}\n").expect("write");
    settle_written_so_far();
    assert_eq!(spells("present", &file), Some(true));
    assert_eq!(reads::of(&file), 1, "the first search reads and keeps");
    assert_eq!(kept_tokens(&file), Some(3));

    assert_eq!(spells("absent", &file), Some(false));
    assert_eq!(spells("presen", &file), Some(false));
    assert_eq!(reads::of(&file), 1, "absent names are answered unread");

    assert_eq!(spells("present", &file), Some(true));
    assert_eq!(reads::of(&file), 2, "a hit is confirmed by the file");
}

/// A hash hit is only a candidate: the file itself decides, so a collision
/// can cost a read and never a false claim.
#[test]
fn a_colliding_hash_answers_from_the_file() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    std::fs::write(&file, "something else\n").expect("write");
    settle_written_so_far();
    let stamp = cache_stamp(&std::fs::metadata(&file).expect("meta")).expect("stamp");
    keep(
        file.to_str().expect("utf-8 path"),
        stamp,
        Box::new([word_hash(b"collides")]),
    );
    assert_eq!(spells("collides", &file), Some(false));
    assert_eq!(reads::of(&file), 1);
}

/// A file inside the settle window can change unseen in one clock tick: it is
/// read every time, and kept once settled.
#[test]
fn an_unsettled_file_is_read_and_not_kept() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    std::fs::write(&file, "word\n").expect("write");
    for reads_so_far in 1..=2 {
        assert_eq!(spells("word", &file), Some(true));
        assert_eq!(kept_tokens(&file), None);
        assert_eq!(reads::of(&file), reads_so_far);
    }
    settle_written_so_far();
    assert_eq!(spells("word", &file), Some(true));
    assert_eq!(kept_tokens(&file), Some(1));
}

/// Every kind of edit reaches the next search, and the entry it left behind
/// is not served again.
#[test]
fn an_edit_reaches_the_next_search() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    std::fs::write(&file, "first\n").expect("write");
    settle_written_so_far();
    assert_eq!(spells("first", &file), Some(true));
    assert_eq!(spells("second", &file), Some(false));

    std::fs::write(&file, "first second\n").expect("write");
    assert_eq!(spells("second", &file), Some(true), "a word added");
    assert_eq!(kept_tokens(&file), None, "the old entry is gone");

    std::fs::write(&file, "second\n").expect("write");
    assert_eq!(spells("first", &file), Some(false), "a word removed");

    settle_written_so_far();
    assert_eq!(spells("second", &file), Some(true));
    assert_eq!(kept_tokens(&file), Some(1));
    assert_eq!(spells("thirds", &file), Some(false));
    std::fs::remove_file(&file).expect("remove");
    assert_eq!(spells("second", &file), Some(false), "a removed file");
}

/// A same-size write that restores the modification time still moves the
/// status change time, and that is enough.
#[cfg(unix)]
#[test]
fn a_same_size_edit_with_its_time_restored_is_seen() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    std::fs::write(&file, "alpha\n").expect("write");
    settle_written_so_far();
    assert_eq!(spells("alpha", &file), Some(true));
    assert_eq!(spells("omega", &file), Some(false));
    let modified = std::fs::metadata(&file)
        .expect("meta")
        .modified()
        .expect("mtime");
    std::fs::write(&file, "omega\n").expect("write");
    std::fs::File::options()
        .write(true)
        .open(&file)
        .expect("open")
        .set_modified(modified)
        .expect("restore mtime");
    assert_eq!(
        std::fs::metadata(&file)
            .expect("meta")
            .modified()
            .expect("mtime"),
        modified
    );
    assert_eq!(spells("omega", &file), Some(true));
    assert_eq!(spells("alpha", &file), Some(false));
}

/// A rename over a kept path brings another inode: its entry does not answer.
#[cfg(unix)]
#[test]
fn a_rename_over_a_kept_path_is_seen() {
    let (_dir, root) = fixture();
    let (file, other) = (root.join("a.ts"), root.join("b.ts"));
    std::fs::write(&file, "before\n").expect("write");
    std::fs::write(&other, "after!\n").expect("write");
    settle_written_so_far();
    assert_eq!(spells("after", &file), Some(false));
    std::fs::rename(&other, &file).expect("rename");
    assert_eq!(spells("after", &file), Some(true));
    assert_eq!(spells("before", &file), Some(false));
}

/// Past one read window a search sees less around each window than a whole
/// text does: such a file keeps the plain search, every time.
#[test]
fn a_file_over_one_window_is_searched_and_not_kept() {
    let (_dir, root) = fixture();
    let file = root.join("big.ts");
    let mut text = "x".repeat(MENTION_CHUNK_BYTES - 2);
    text.push_str(" target();\n");
    std::fs::write(&file, &text).expect("write");
    settle_written_so_far();
    for round in 1..=2 {
        assert_eq!(spells("target", &file), Some(true));
        assert_eq!(spells("absent", &file), Some(false));
        assert_eq!(reads::of(&file), 2 * round);
    }
    assert_eq!(kept_tokens(&file), None);
    // A NUL past the first window decides by its own window.
    let mut bytes = "y ".repeat(MENTION_CHUNK_BYTES).into_bytes();
    bytes.extend_from_slice(b"late\0more");
    std::fs::write(&file, &bytes).expect("write");
    settle_written_so_far();
    assert_eq!(spells("late", &file), file_spells(&file, b"late", None));
    assert_eq!(spells("more", &file), Some(false));
    assert_eq!(kept_tokens(&file), None);
}

/// A name that is not one token is searched for in the text, kept or not.
#[test]
fn a_name_that_is_not_an_identifier_keeps_the_plain_search() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    std::fs::write(&file, "state.name(); a::b\n").expect("write");
    settle_written_so_far();
    assert_eq!(spells("state", &file), Some(true));
    assert_eq!(kept_tokens(&file), Some(4));
    let before = reads::of(&file);
    for (name, expected) in [
        (".name", true),
        ("a::b", true),
        (".nam", false),
        ("", false),
    ] {
        assert_eq!(spells(name, &file), Some(expected), "{name:?}");
        assert_eq!(file_spells(&file, name.as_bytes(), None), Some(expected));
    }
    assert_eq!(reads::of(&file), before + 4);
}

/// The size bound and an unreadable file answer as the plain search does.
#[test]
fn the_size_bound_and_unreadable_files_answer_as_the_plain_search() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    std::fs::write(&file, "word\n").expect("write");
    settle_written_so_far();
    let bounded = |limit| WordSearch::new("word", Some(limit), settle_clock());
    assert_eq!(bounded(4).file_spells(&file), Some(false));
    assert_eq!(reads::of(&file), 0, "over the bound, unread");
    assert_eq!(bounded(5).file_spells(&file), Some(true));
    // A name that is no identifier meets the same bound, from the open file.
    let dotted = |limit| WordSearch::new("word\n", Some(limit), settle_clock());
    assert_eq!(dotted(4).file_spells(&file), Some(false));
    assert_eq!(dotted(5).file_spells(&file), Some(true));
    assert_eq!(spells("word", &root.join("missing.ts")), Some(false));
    assert_eq!(spells("word", &root), None);
}

/// An entry weighs its tokens against the one budget, and leaves with them.
#[test]
fn an_entry_weighs_its_tokens() {
    let (_dir, root) = fixture();
    let file = root.join("a.ts");
    let words = (0..20_000).map(|n| format!("w{n}")).collect::<Vec<_>>();
    std::fs::write(&file, words.join(" ")).expect("write");
    settle_written_so_far();
    let key = file.to_str().expect("utf-8 path");
    assert_eq!(spells("w7", &file), Some(true));
    assert_eq!(kept_tokens(&file), Some(20_000));
    let (keys, held) = SPELLED.contents();
    assert!(keys.iter().any(|kept| kept == key));
    assert!(held >= 20_000 * std::mem::size_of::<u64>(), "{held}");
    SPELLED.evict(key);
    assert_eq!(kept_tokens(&file), None);
    assert_eq!(spells("w7", &file), Some(true), "an evicted file is read");
}

/// Each kept file was read whole, whatever stopped the scan: what a stopped
/// scan left answers as a complete one does.
#[test]
fn a_stopped_scan_keeps_only_whole_files() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_dir, root) = fixture();
    let files = (0..8)
        .map(|n| {
            let file = root.join(format!("f{n}.ts"));
            std::fs::write(&file, format!("shared unique{n}\n")).expect("write");
            file
        })
        .collect::<Vec<_>>();
    settle_written_so_far();
    let clock = settle_clock();
    let checks = AtomicUsize::new(0);
    let stopped = || checks.fetch_add(1, Ordering::Relaxed) >= 3;
    assert_eq!(
        super::super::listing::spelling(&files, "shared", clock, &stopped),
        None,
        "a stopped scan has no answer"
    );
    let kept = files
        .iter()
        .filter_map(|file| kept_tokens(file))
        .collect::<Vec<_>>();
    assert!(!kept.is_empty() && kept.len() < files.len(), "{kept:?}");
    assert!(kept.iter().all(|tokens| *tokens == 2), "{kept:?}");

    let spelled = super::super::listing::spelling(&files, "unique5", clock, &|| false);
    let expected = BTreeSet::from([files[5].to_string_lossy().into_owned()]);
    assert_eq!(spelled, Some(expected));
    let all = super::super::listing::spelling(&files, "shared", clock, &|| false);
    assert_eq!(all.map(|found| found.len()), Some(files.len()));
}
