use super::*;
use octocode_engine::types::FileKind;

/// A kept listing draws on the one budget for all it holds: its table and
/// every string of every entry, not the rows of one window.
#[test]
fn a_kept_listing_is_weighed_by_the_heap_it_holds() {
    let text = "x".repeat(100);
    let entry = FileSystemEntry {
        path: text.clone(),
        relative_path: text.clone(),
        name: text,
        entry_type: FileKind::File,
        size: None,
        modified_time: None,
        walk_position: 1,
        stamp: None,
    };
    let meta = |entries: Vec<FileSystemEntry>| Ranked {
        lines: Vec::new(),
        meta: Walked::served(Vec::new(), &empty_meta(), 0, Proof::default()).meta,
        dirs: Proof::default(),
        entries,
    };
    let none = meta(Vec::new()).weight();
    let thousand = meta(vec![entry; 1000]).weight() - none;
    assert!(
        thousand >= 1000 * (300 + size_of::<FileSystemEntry>()),
        "{thousand}"
    );
}

fn empty_meta() -> WalkMeta {
    WalkMeta {
        covered: 0,
        available: 0,
        total_discovered: 0,
        was_capped: false,
        time_cut: None,
        time_window: super::super::cut::TimeWindow::Whole,
        walk_position: 0,
        files: 0,
        dirs: 0,
        bytes: None,
        uncovered: super::super::probe::Uncovered::default(),
        walked_dirs: Vec::new(),
        names_only: true,
        test_code: None,
        verdicts: super::super::walked::Verdicts::Unscoped,
        pinned_before: None,
        pins_through: None,
    }
}
