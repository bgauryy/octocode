//! A hit's declaration class is that of its whole line, whatever window shows it.

use super::*;

/// A hit on a line past [`crate::security::LONG_LINE_BYTES`] shows a window of
/// the line, but its rank and byte offset are the whole line's: a hit deep in
/// a long comment ranks as prose, at the hit's own byte.
#[test]
fn a_windowed_long_line_row_ranks_and_offsets_as_its_whole_line() {
    let t = TmpDir::new();
    let filler = "x".repeat(crate::security::LONG_LINE_BYTES + 1);
    let line = format!("// {filler} needle\n");
    t.write("min.rs", &line);
    let mut o = opts(t.path(), "needle");
    // Context lines keep the row on the assembled (not deferred) path.
    o.context = crate::types::SearchContext::symmetric(1);
    let r = search(o).expect("ok");
    let hit = &r.files[0].matches[0];
    assert!(
        hit.value.len() < line.len(),
        "the row is a window of the line"
    );
    assert_eq!(hit.byte_offset as usize, filler.len() + 4);
    assert_eq!(hit.rank, Some(0), "a hit in a comment ranks as prose");
}
