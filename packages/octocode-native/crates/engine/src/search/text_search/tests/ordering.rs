//! Result order and the collection cap.

use super::*;

#[test]
fn caps_collected_files_and_reports_non_exhaustive_stats() {
    let t = TmpDir::new();
    t.write("a.txt", "needle\n");
    t.write("b.txt", "needle\n");
    t.write("c.txt", "needle\n");
    let mut o = opts(t.path(), "needle");
    o.max_collected_files = Some(2);

    let r = search(o).expect("ok");

    assert_eq!(r.files.len(), 2);
    assert_eq!(r.stats.capped, Some(true));
    assert_eq!(r.stats.cap_reasons, [CapReason::MaxCollectedFiles]);
}

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
fn sort_then_cap_retains_deterministic_sorted_prefix() {
    // The collection cap must be a STABLE truncation of the sorted prefix,
    // not a race-dependent subset chosen during the parallel walk.
    // Feeding an unsorted record set exceeding a small cap must always retain
    // the same sorted prefix, identical across repeated runs.
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
        }
    }
    let make = || {
        vec![
            rec("m.txt"),
            rec("a.txt"),
            rec("z.txt"),
            rec("b.txt"),
            rec("c.txt"),
        ]
    };
    let mut o = opts("/fixture".to_owned(), "p");
    o.max_collected_files = Some(3);
    let run = || {
        let mut recs = make();
        let capped = sort_and_cap(&o, Mode::Normal, &mut recs);
        (
            recs.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            capped,
        )
    };
    let (first, capped_first) = run();
    assert_eq!(first, vec!["a.txt", "b.txt", "c.txt"]);
    assert!(capped_first);
    for _ in 0..10 {
        let (again, capped_again) = run();
        assert_eq!(again, first, "sorted-prefix truncation must be stable");
        assert!(capped_again);
    }

    // Under the cap: no truncation, all records kept in sorted order.
    let mut under = vec![rec("b.txt"), rec("a.txt")];
    let mut o2 = opts("/fixture".to_owned(), "p");
    o2.max_collected_files = Some(3);
    assert!(!sort_and_cap(&o2, Mode::Normal, &mut under));
    assert_eq!(
        under.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
        vec!["a.txt", "b.txt"]
    );
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
fn match_count_order_keeps_the_most_matched_file_past_the_path_prefix() {
    let t = TmpDir::new();
    for i in 0..30 {
        t.write(&format!("a{i:03}.txt"), "hit\n");
    }
    t.write("zzz-hot.txt", "hit\n".repeat(40));
    let mut o = opts(t.path(), "hit");
    o.sort = SearchSort::MatchCount;
    o.max_collected_files = Some(10);
    let r = search(o.clone()).expect("ok");
    assert_eq!(r.files.len(), 10);
    assert!(
        r.files[0].path.ends_with("zzz-hot.txt"),
        "{}",
        r.files[0].path
    );
    // Ties keep ascending path order, so the retained set is deterministic.
    assert!(r.files[1].path.ends_with("a000.txt"));
    assert!(r.files[9].path.ends_with("a008.txt"));
    // Totals count every matched file, not only the retained ones.
    assert_eq!(r.stats.files_matched, Some(31));
    assert_eq!(r.stats.match_count, Some(70));
    assert_eq!(r.stats.cap_reasons, [CapReason::MaxCollectedFiles]);
    for _ in 0..3 {
        let again = search(o.clone()).expect("ok");
        let paths =
            |files: &[TextSearchFile]| files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&again.files), paths(&r.files));
    }
}

#[test]
fn bounded_retention_matches_a_full_sort() {
    // Worker top-k retention must keep the same records as one full sort under
    // every density order, including relevance's secondary keys.
    fn rec(path: String, hits: u32, salt: u32) -> FileRec {
        FileRec {
            path,
            entry: FileEntry::new(),
            matched_lines: hits,
            submatches: hits,
            om_matches: Vec::new(),
            sort_time: None,
            line_weight: salt % 5,
            demoted: salt.is_multiple_of(3),
            generated: salt.is_multiple_of(7),
            declares: salt.is_multiple_of(4),
            covered: 0,
            source: None,
        }
    }
    for sort in [SearchSort::MatchCount, SearchSort::Relevance] {
        let mut o = opts("/fixture".to_owned(), "p");
        o.sort = sort;
        let all: Vec<(String, u32, u32)> = (0..200u32)
            .map(|i| (format!("f{:03}", (i * 37) % 200), (i * 13) % 7, i))
            .collect();
        let mut full: Vec<FileRec> = all
            .iter()
            .map(|(p, h, salt)| rec(p.clone(), *h, *salt))
            .collect();
        sort_recs(&o, Mode::Normal, &mut full);
        full.truncate(5);
        let mut bounded = Vec::new();
        for (p, h, salt) in &all {
            retain_into(
                &o,
                Mode::Normal,
                Some(5),
                &mut bounded,
                rec(p.clone(), *h, *salt),
            );
        }
        sort_and_cap(
            &{
                let mut capped = o.clone();
                capped.max_collected_files = Some(5);
                capped
            },
            Mode::Normal,
            &mut bounded,
        );
        let paths = |recs: &[FileRec]| recs.iter().map(|r| r.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&bounded), paths(&full), "{sort:?}");
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
    assert_eq!(saturate_u32(u64::from(u32::MAX) * 2), u32::MAX);
}
