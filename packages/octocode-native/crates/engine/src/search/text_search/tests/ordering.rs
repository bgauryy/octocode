//! Result order.

use super::*;

#[test]
fn results_are_sorted_by_path() {
    let t = TmpDir::new();
    t.write("z.txt", "m\n");
    t.write("a.txt", "m\n");
    t.write("m.txt", "m\n");
    let r = search(opts(t.path(), "m")).expect("ok");
    let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);
}

#[test]
fn sorting_an_unordered_record_set_is_deterministic() {
    // Parallel workers merge in a race-dependent order; the final sort must
    // give the same path order on every run.
    fn rec(path: &str) -> FileRec {
        FileRec {
            path: path.to_owned(),
            entry: FileEntry::new(),
            matched_lines: 1,
            submatches: 1,
            om_matches: Vec::new(),
            sort_time: None,
            line_weight: 0,
            demoted: false,
            generated: false,
            declares: false,
            covered: 0,
            source: None,
            not_utf8: false,
            term_counts: Vec::new(),
        }
    }
    let o = opts("/fixture".to_owned(), "p");
    let run = || {
        let mut recs = vec![
            rec("m.txt"),
            rec("a.txt"),
            rec("z.txt"),
            rec("b.txt"),
            rec("c.txt"),
        ];
        sort_recs(&o, Mode::Normal, &mut recs);
        recs.iter().map(|r| r.path.clone()).collect::<Vec<_>>()
    };
    let first = run();
    assert_eq!(first, vec!["a.txt", "b.txt", "c.txt", "m.txt", "z.txt"]);
    for _ in 0..10 {
        assert_eq!(run(), first);
    }
}

#[test]
fn traversal_sort_output_is_stable_across_runs() {
    // With sort:"traversal" the walk is forced single-threaded so the
    // emitted order is stable run-to-run on the same tree.
    let t = TmpDir::new();
    for i in 0..40 {
        t.write(&format!("dir{}/file{i:02}.txt", i % 5), "needle\n");
    }
    let run = || {
        let mut o = opts(t.path(), "needle");
        o.sort = SearchSort::Traversal;
        search(o)
            .expect("ok")
            .files
            .into_iter()
            .map(|f| f.path)
            .collect::<Vec<_>>()
    };
    let first = run();
    assert_eq!(first.len(), 40);
    for _ in 0..5 {
        assert_eq!(run(), first, "traversal order must be stable across runs");
    }
}

#[test]
fn explicit_traversal_sort_bypasses_post_collection_sorting() {
    let mut o = opts("/fixture".to_owned(), "m");
    o.sort = SearchSort::Traversal;
    assert!(preserves_traversal_order(&o));
    o.sort = SearchSort::Path;
    assert!(!preserves_traversal_order(&o));
}

#[test]
fn match_count_order_ranks_the_most_matched_file_first_and_keeps_every_file() {
    let t = TmpDir::new();
    for i in 0..30 {
        t.write(&format!("a{i:03}.txt"), "hit\n");
    }
    t.write("zzz-hot.txt", "hit\n".repeat(40));
    let mut o = opts(t.path(), "hit");
    o.sort = SearchSort::MatchCount;
    let r = search(o.clone()).expect("ok");
    assert_eq!(r.files.len(), 31);
    assert!(
        r.files[0].path.ends_with("zzz-hot.txt"),
        "{}",
        r.files[0].path
    );
    // Ties keep ascending path order, so the order is deterministic.
    assert!(r.files[1].path.ends_with("a000.txt"));
    assert!(r.files[30].path.ends_with("a029.txt"));
    assert_eq!(r.stats.files_matched, Some(31));
    assert_eq!(r.stats.match_count, Some(70));
    assert!(r.stats.cap_reasons.is_empty());
    for _ in 0..3 {
        let again = search(o.clone()).expect("ok");
        let paths =
            |files: &[TextSearchFile]| files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&again.files), paths(&r.files));
    }
}

#[test]
fn aggregate_totals_saturate_instead_of_overflowing() {
    let state = CollectState::new();
    state.record_kept(u32::MAX, u32::MAX);
    state.record_kept(u32::MAX, u32::MAX);
    let r = build_result(
        &opts("/fixture".to_owned(), "p"),
        Mode::CountMatches,
        state.snapshot(),
    );
    assert_eq!(r.stats.match_count, Some(u32::MAX));
    assert_eq!(r.stats.matched_lines, Some(u32::MAX));
    assert_eq!(r.stats.files_matched, Some(2));
}
