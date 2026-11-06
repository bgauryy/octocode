//! A PCRE2 search in windows: each pauses after the last file finished in
//! walk order, and the window that ends the search returns what one unpaused
//! search returns, whatever the clock did and whether the paused windows'
//! files were kept.
use super::*;
use std::time::{Duration, Instant};

/// The time a test clock advances at each file a window finishes: far past
/// any real run, so only the injected clock ends a window.
const STEP: Duration = Duration::from_secs(3600);

/// A window that ends once `files` files past where it resumes are
/// finished, on every run and machine (`None`: no end).
fn window(files: Option<u32>, resume: Option<Resume>) -> SearchWindow {
    let Some(files) = files else {
        return SearchWindow {
            resume,
            ..SearchWindow::default()
        };
    };
    let started = Instant::now();
    let read = Arc::new(AtomicU32::new(0));
    SearchWindow {
        ends: Some(started + STEP * files),
        clock: Some(Arc::new(move || {
            started + STEP * (read.fetch_add(1, Ordering::Relaxed) + 1)
        })),
        resume,
    }
}

/// Fifteen files over nested directories, with match counts that rank in
/// another order than their paths, one without a match.
fn tree() -> TmpDir {
    let t = TmpDir::new();
    for index in 0..15_usize {
        let dir = ["a", "b", "c/d", "c"][index % 4];
        let hits = (index * 7) % 5;
        let body = format!("{}plain line\n", "foo bar\n".repeat(hits));
        t.write(&format!("{dir}/f{index:02}.txt"), &body);
    }
    t
}

fn pcre2(root: String, sort: SearchSort) -> TextSearchOptions {
    let mut o = opts(root, "fo(?=o)");
    o.syntax = PatternSyntax::Pcre2;
    o.sort = sort;
    o
}

/// Each file's path, match count and hit lines, then the search's totals.
type Facts = (Vec<(String, u32, Vec<u32>)>, Vec<Option<u32>>);

/// What a result says, comparable across runs.
fn facts(result: &TextSearchResult) -> Facts {
    let files = result
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                file.match_count,
                file.matches.iter().map(|hit| hit.line).collect(),
            )
        })
        .collect();
    let stats = &result.stats;
    (
        files,
        vec![
            stats.match_count,
            stats.matched_lines,
            stats.files_matched,
            stats.files_searched,
            stats.error_count,
        ],
    )
}

/// Every window from the first to the one that ends the search, each ending
/// after the next count of `budgets` files (the last repeats); `keep` hands
/// each paused window's files to the next. Returns the result and the
/// windows that paused.
fn chain(
    o: &TextSearchOptions,
    filter: &Arc<dyn TextSearchPathFilter>,
    budgets: &[Option<u32>],
    keep: bool,
) -> (TextSearchResult, usize) {
    let (mut resume, mut floor) = (None, 0);
    for call in 0..100 {
        let budget = budgets[call.min(budgets.len() - 1)];
        match search_window(
            o.clone(),
            Arc::clone(filter),
            &|| false,
            window(budget, resume),
        )
        .expect("window")
        {
            Windowed::Whole(result) => return (result, call),
            Windowed::Paused(pause) => {
                assert!(pause.position > floor, "a window makes progress");
                floor = pause.position;
                assert_eq!(
                    pause.searched.files_searched(),
                    u64::from(pause.position),
                    "every file before the pause, and none after it"
                );
                resume = Some(Resume {
                    position: pause.position,
                    earlier: keep.then_some(Earlier {
                        pin: pause.pin,
                        searched: pause.searched,
                    }),
                });
            }
        }
    }
    panic!("the search never ends");
}

#[test]
fn a_paused_pcre2_search_resumed_to_its_end_equals_one_unpaused_search() {
    let t = tree();
    let filter: Arc<dyn TextSearchPathFilter> = Arc::new(AllowAll);
    let schedules: [&[Option<u32>]; 5] = [
        &[Some(1)],
        &[Some(2)],
        &[Some(5)],
        &[Some(3), None],
        &[None, Some(2)],
    ];
    for sort in [
        SearchSort::MatchCount,
        SearchSort::Relevance,
        SearchSort::Path,
        SearchSort::Traversal,
    ] {
        let o = pcre2(t.path(), sort);
        let whole = search(o.clone()).expect("unpaused");
        assert_eq!(whole.files.len(), 12, "{sort:?}: {:?}", whole.stats);
        for schedule in schedules {
            for keep in [true, false] {
                let (result, paused) = chain(&o, &filter, schedule, keep);
                assert_eq!(
                    facts(&result),
                    facts(&whole),
                    "{sort:?} {schedule:?} keep={keep}"
                );
                assert!(
                    paused > 0 || schedule[0].is_none(),
                    "{sort:?} {schedule:?}: no window paused"
                );
            }
        }
    }
}

/// Calls `on_file` as each file's search starts: the test-code hook every
/// searched file passes, with nothing left out.
fn hooked(o: &mut TextSearchOptions, on_file: impl Fn(&Path) + Send + Sync + 'static) {
    o.test_code = Some(crate::types::TestCodeScope {
        keep_test: false,
        test_path: Arc::new(|_| false),
        unread_test_path: Arc::new(move |path| {
            on_file(path);
            false
        }),
        may_hold: Arc::new(|_| false),
        spans: Arc::new(|_| Arc::new(Vec::new())),
    });
}

fn stalls_on(name: &'static str, stall: Duration) -> impl Fn(&Path) + Send + Sync {
    move |path| {
        if path.ends_with(name) {
            std::thread::sleep(stall);
        }
    }
}

/// Files finished after a slow one wait for it: the pause comes after the
/// last file finished in walk order, never past a gap.
#[test]
fn a_window_pauses_after_the_last_file_finished_in_walk_order() {
    let t = tree();
    let mut o = pcre2(t.path(), SearchSort::Path);
    hooked(&mut o, stalls_on("a/f04.txt", Duration::from_millis(200)));
    let filter: Arc<dyn TextSearchPathFilter> = Arc::new(AllowAll);
    let Windowed::Paused(pause) = search_window(
        o.clone(),
        Arc::clone(&filter),
        &|| false,
        window(Some(3), None),
    )
    .expect("window") else {
        panic!("three files of fifteen end the window");
    };
    assert_eq!(pause.position, 3);
    assert_eq!(pause.searched.files_searched(), 3);
    let resume = Resume {
        position: pause.position,
        earlier: Some(Earlier {
            pin: pause.pin,
            searched: pause.searched,
        }),
    };
    let Windowed::Whole(result) =
        search_window(o.clone(), filter, &|| false, window(None, Some(resume))).expect("window")
    else {
        panic!("a window with no end ends the search");
    };
    assert_eq!(facts(&result), facts(&search(o).expect("unpaused")));
}

/// What earlier windows kept is held from the start of the window that
/// resumes with it: a resumed search whose later files hold nothing is
/// refused when the kept files alone pass its bound, and served whole at
/// that bound.
#[test]
fn a_resumed_window_holds_what_earlier_windows_kept() {
    let t = TmpDir::new();
    for name in ["a", "b", "c"] {
        t.write(&format!("{name}.txt"), "foo bar\n".repeat(50));
    }
    for name in ["d", "e", "f", "g"] {
        t.write(&format!("{name}.txt"), "plain line\n");
    }
    let o = pcre2(t.path(), SearchSort::Path);
    let filter: Arc<dyn TextSearchPathFilter> = Arc::new(AllowAll);
    let Windowed::Paused(pause) = search_window(
        o.clone(),
        Arc::clone(&filter),
        &|| false,
        window(Some(3), None),
    )
    .expect("window") else {
        panic!("three files of seven end the window");
    };
    assert_eq!(pause.position, 3);
    let kept = pause.searched.weight();
    let resumed = |bound: usize, pin: String, searched| {
        let mut bounded = o.clone();
        bounded.max_held_bytes = Some(bound);
        let resume = Resume {
            position: 3,
            earlier: Some(Earlier { pin, searched }),
        };
        search_window(
            bounded,
            Arc::clone(&filter),
            &|| false,
            window(None, Some(resume)),
        )
    };
    let refused = resumed(kept - 1, pause.pin.clone(), pause.searched)
        .err()
        .expect("the kept files pass the bound");
    assert_eq!(
        *refused.kind(),
        crate::error::ErrorKind::ResourceExhausted,
        "{refused}"
    );
    let Windowed::Paused(again) = search_window(
        o.clone(),
        Arc::clone(&filter),
        &|| false,
        window(Some(3), None),
    )
    .expect("window") else {
        panic!("three files of seven end the window");
    };
    let Windowed::Whole(result) = resumed(kept, again.pin, again.searched).expect("fits") else {
        panic!("a window with no end ends the search");
    };
    assert_eq!(facts(&result), facts(&search(o).expect("unpaused")));
}

/// A kept window serves only while the files it passed are unchanged: an
/// edit among them searches them again.
#[test]
fn a_kept_window_whose_files_changed_is_searched_again() {
    let t = tree();
    let o = pcre2(t.path(), SearchSort::MatchCount);
    let filter: Arc<dyn TextSearchPathFilter> = Arc::new(AllowAll);
    let Windowed::Paused(pause) = search_window(
        o.clone(),
        Arc::clone(&filter),
        &|| false,
        window(Some(4), None),
    )
    .expect("window") else {
        panic!("four files of fifteen end the window");
    };
    // The first file in walk order now holds five more hits.
    std::thread::sleep(Duration::from_millis(20));
    t.write("a/f00.txt", "foo\n".repeat(5));
    let resume = Resume {
        position: pause.position,
        earlier: Some(Earlier {
            pin: pause.pin,
            searched: pause.searched,
        }),
    };
    let Windowed::Whole(result) =
        search_window(o.clone(), filter, &|| false, window(None, Some(resume))).expect("window")
    else {
        panic!("a window with no end ends the search");
    };
    assert_eq!(facts(&result), facts(&search(o).expect("unpaused")));
}

/// A match no file boundary interrupts does not hold the answer: past the
/// window's end the driver pauses after the files finished before it.
#[test]
fn a_window_stuck_in_one_file_pauses_at_its_end_without_waiting() {
    let t = TmpDir::new();
    for i in 0..6 {
        t.write(&format!("f{i}.txt"), "foo\n");
    }
    let mut o = pcre2(t.path(), SearchSort::Path);
    hooked(&mut o, stalls_on("f3.txt", Duration::from_millis(1500)));
    let started = Instant::now();
    let ends = SearchWindow {
        ends: Some(started + Duration::from_millis(100)),
        ..SearchWindow::default()
    };
    let windowed = search_window(o, Arc::new(AllowAll), &|| false, ends).expect("window");
    assert!(
        started.elapsed() < Duration::from_millis(1200),
        "the driver must not wait for the stuck worker"
    );
    let Windowed::Paused(pause) = windowed else {
        panic!("the window ended inside f3.txt");
    };
    assert_eq!(pause.position, 3);
    assert_eq!(pause.searched.files_searched(), 3);
}

#[test]
fn pcre2_driver_honours_cancellation_while_the_worker_is_stuck() {
    let t = TmpDir::new();
    for i in 0..4 {
        t.write(&format!("f{i}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.syntax = PatternSyntax::Pcre2;
    o.sort = SearchSort::Path;
    hooked(&mut o, stalls_on("f1.txt", Duration::from_millis(1500)));
    let started = Instant::now();
    let cancel = || started.elapsed() >= Duration::from_millis(100);
    let r = search_cancellable(o, Arc::new(AllowAll), &cancel).expect("ok");
    assert!(started.elapsed() < Duration::from_millis(1200));
    // Only the file before the stuck one is committed in walk order.
    assert_eq!(r.files.len(), 1, "{:?}", r.stats);
    assert!(r.stats.cap_reasons.contains(&CapReason::Cancelled));
}
