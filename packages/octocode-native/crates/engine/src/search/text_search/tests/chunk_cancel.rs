//! A cancelled search leaves the file it is inside at the next chunk of its
//! work, not at the end of the file.

use super::*;

fn counted(root: String, pattern: &str, cancel_after: u32) -> (u32, u32) {
    let mut options = opts(root, pattern);
    options.output = SearchOutput::CountMatches;
    options.sort = SearchSort::Traversal;
    let polls = AtomicU32::new(0);
    let cancel = || polls.fetch_add(1, Ordering::SeqCst) >= cancel_after;
    let result = search_cancellable(options, Arc::new(AllowAll), &cancel).expect("search");
    (
        result.stats.match_count.unwrap_or(0),
        polls.load(Ordering::SeqCst),
    )
}

/// A file of many matched lines.
#[test]
fn a_cancel_ends_the_matched_lines_of_one_file() {
    let t = TmpDir::new();
    let lines = 400_000;
    t.write("big.txt", &"needle\n".repeat(lines));
    let (found, _) = counted(t.path(), "needle", 3);
    assert!(
        found < lines as u32 / 2,
        "the whole file was searched: {found}"
    );
    let (found, _) = counted(t.path(), "needle", u32::MAX);
    assert_eq!(
        found, lines as u32,
        "an uncancelled search counts every match"
    );
}

/// One line holding many matches: the count walks them in chunks.
#[test]
fn a_cancel_ends_the_matches_of_one_long_line() {
    let t = TmpDir::new();
    let hits = 300_000;
    t.write("line.txt", &format!("{}\n", "needle ".repeat(hits)));
    let (found, _) = counted(t.path(), "needle", 3);
    assert!(
        found < hits as u32 / 2,
        "the whole line was counted: {found}"
    );
    let (found, _) = counted(t.path(), "needle", u32::MAX);
    assert_eq!(found, hits as u32);
}

/// A file whose line outgrows the line buffer is read whole in chunks that
/// end with the search.
#[test]
fn a_cancel_ends_the_read_of_a_file_held_whole() {
    let t = TmpDir::new();
    let hits = 12_000_000;
    t.write("huge.txt", &format!("{}\n", "needle ".repeat(hits)));
    let (found, _) = counted(t.path(), "needle", 3);
    assert!(
        found < hits as u32 / 2,
        "the whole line was counted: {found}"
    );
}
