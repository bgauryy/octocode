//! The bytes workers hold for whole-file work stay within the budget however
//! many workers run, and a stopped search ends a judge at work.

use super::*;
use crate::memory::{WHOLE_FILES, test_limit};
use crate::types::SourceJudge;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

const FILES: usize = 16;

/// A judge that serves each file with `needle` shown as `NEEDLE`, keeps each
/// call open for a moment so workers overlap, and counts how many ran at once.
fn slow_judge(now: &Arc<AtomicUsize>, most: &Arc<AtomicUsize>) -> SourceJudge {
    let (now, most) = (Arc::clone(now), Arc::clone(most));
    SourceJudge {
        serve: Arc::new(
            move |_: &Path, file: &std::fs::File, _: &(dyn Fn() -> bool + Sync)| {
                use std::io::Read as _;
                let running = now.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(running, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                let mut text = String::new();
                (&*file).read_to_string(&mut text)?;
                now.fetch_sub(1, Ordering::SeqCst);
                // Served as the source's own text: nothing of it written.
                Ok(Some(crate::types::ServedText {
                    text: text.replace("needle", "NEEDLE").into_bytes(),
                    written: Vec::new(),
                }))
            },
        ),
        writes: Arc::new(|_: &str, _: bool| true),
        held: None,
    }
}

fn tree() -> (TmpDir, usize) {
    let root = TmpDir::new();
    let body = format!("{}needle\n", "x".repeat(32 * 1024));
    for index in 0..FILES {
        root.write(&format!("f{index:02}.txt"), &body);
    }
    (root, body.len())
}

/// The rows of a search of the tree for `NEEDLE`, and the most judge calls at once.
fn rows(root: &TmpDir) -> (Vec<(String, u32)>, usize) {
    let (now, most) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut options = opts(root.path(), "NEEDLE");
    options.sort = SearchSort::Path;
    options.judge = Some(slow_judge(&now, &most));
    let result = search(options).expect("search");
    let rows = result
        .files
        .iter()
        .map(|file| (file.path.clone(), file.match_count))
        .collect();
    (rows, most.load(Ordering::SeqCst))
}

#[test]
fn held_bytes_stay_within_the_budget_whatever_the_workers() {
    let (root, len) = tree();
    let limit = test_limit::hold(usize::MAX / 4);
    let (unbounded, widest) = rows(&root);
    assert_eq!(unbounded.len(), FILES);
    if crate::cpu::host().share() >= 4 {
        assert!(widest > 3, "the workers overlap without a budget: {widest}");
    }

    // Room for three files' judge copies at once.
    let budget = 3 * 3 * len;
    limit.set(budget);
    WHOLE_FILES.reset_peak();
    let (bounded, widest) = rows(&root);
    assert!(
        WHOLE_FILES.peak() <= budget,
        "admitted {} of {budget}",
        WHOLE_FILES.peak()
    );
    assert!(widest <= 3, "{widest} judges ran at once");
    assert_eq!(bounded, unbounded);
}

#[test]
fn a_file_larger_than_the_budget_is_searched_alone_not_refused() {
    let (root, len) = tree();
    let _limit = test_limit::hold(len / 2);
    WHOLE_FILES.reset_peak();
    let (bounded, widest) = rows(&root);
    assert_eq!(bounded.len(), FILES);
    assert_eq!(widest, 1);
    assert!(WHOLE_FILES.peak() <= len / 2);
}

#[test]
fn a_stopped_search_ends_a_judge_at_work() {
    let root = TmpDir::new();
    root.write("only.txt", "needle\n");
    let started = Arc::new(AtomicBool::new(false));
    let saw_stop = Arc::new(AtomicBool::new(false));
    let (flag, seen) = (Arc::clone(&started), Arc::clone(&saw_stop));
    let mut options = opts(root.path(), "NEEDLE");
    options.judge = Some(SourceJudge {
        serve: Arc::new(
            move |_: &Path, _: &std::fs::File, halted: &(dyn Fn() -> bool + Sync)| {
                flag.store(true, Ordering::SeqCst);
                let until = std::time::Instant::now() + Duration::from_secs(10);
                while std::time::Instant::now() < until {
                    if halted() {
                        seen.store(true, Ordering::SeqCst);
                        return Err(std::io::ErrorKind::Interrupted.into());
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(None)
            },
        ),
        writes: Arc::new(|_: &str, _: bool| true),
        held: None,
    });
    let cancel = Arc::new(AtomicBool::new(false));
    let canceller = {
        let (started, cancel) = (Arc::clone(&started), Arc::clone(&cancel));
        std::thread::spawn(move || {
            while !started.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(2));
            }
            cancel.store(true, Ordering::SeqCst);
        })
    };
    let begun = std::time::Instant::now();
    let result = search_cancellable(options, Arc::new(AllowAll), &|| {
        cancel.load(Ordering::SeqCst)
    })
    .expect("search");
    canceller.join().expect("canceller");
    assert!(
        saw_stop.load(Ordering::SeqCst),
        "the judge never saw the stop"
    );
    assert!(begun.elapsed() < Duration::from_secs(5));
    assert!(result.files.is_empty());
    // The stop is no failure of the file.
    assert_eq!(result.stats.error_count.unwrap_or(0), 0);
}

/// Files past the line buffer's unit are read under the budget too: a
/// search with no judge holds no more than it admits at once.
#[test]
fn files_read_through_the_line_buffer_are_admitted_by_their_bytes() {
    let root = TmpDir::new();
    let body = format!("{}needle\n", "x".repeat(3 * 512 * 1024));
    for index in 0..FILES {
        root.write(&format!("big{index:02}.txt"), &body);
    }
    let limit = test_limit::hold(usize::MAX / 4);
    let mut options = opts(root.path(), "needle");
    options.sort = SearchSort::Path;
    let unbounded = search(options.clone()).expect("search");

    let budget = 2 * body.len();
    limit.set(budget);
    WHOLE_FILES.reset_peak();
    let bounded = search(options).expect("search");
    assert!(
        WHOLE_FILES.peak() <= budget,
        "admitted {}",
        WHOLE_FILES.peak()
    );
    assert!(WHOLE_FILES.peak() > 0, "the reads were admitted");
    let paths = |result: &TextSearchResult| {
        result
            .files
            .iter()
            .map(|file| (file.path.clone(), file.match_count))
            .collect::<Vec<_>>()
    };
    assert_eq!(paths(&bounded), paths(&unbounded));
    assert_eq!(bounded.files.len(), FILES);
}

/// A file opening with a line past the line buffer's first unit is held whole
/// at once; its matches, in that line and the lines after it, are the same as
/// when the line buffer reads it.
#[test]
fn a_file_opening_with_a_long_line_is_searched_like_any_other() {
    let t = TmpDir::new();
    let long = format!("{}needle\n", "lorem ".repeat(400_000));
    t.write("long.txt", format!("{long}tail needle\nplain\n"));
    t.write("short.txt", format!("head\n{long}tail needle\n"));
    let mut options = opts(t.path(), "needle");
    options.output = SearchOutput::CountMatches;
    let result = search(options).expect("search");
    let counts = result
        .files
        .iter()
        .map(|file| {
            (
                file.path.rsplit('/').next().unwrap_or("").to_owned(),
                file.match_count,
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(counts.get("long.txt"), Some(&2), "{counts:?}");
    assert_eq!(counts.get("short.txt"), Some(&2), "{counts:?}");
}
