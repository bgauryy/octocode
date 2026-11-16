//! A traversal stream stops when its page is full and resumes exactly.

use super::*;
use crate::digest::{ListedFiles, stamp_of};

/// Counts the files the walk handed to a search.
#[derive(Default)]
struct Counting(AtomicU32);

impl TextSearchPathFilter for Counting {
    fn allows(&self, _: &Path, is_dir: bool) -> bool {
        if !is_dir {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        true
    }
}

pub(super) fn traversal(root: &TmpDir, pattern: &str) -> TextSearchOptions {
    let mut o = opts(root.path(), pattern);
    o.sort = SearchSort::Traversal;
    o
}

/// One stream of a page of `files` files: the file after them overflows, and
/// is read too.
pub(super) fn page_with(
    o: &TextSearchOptions,
    filter: Arc<dyn TextSearchPathFilter>,
    from: u32,
    pin: Option<Pin>,
    files: usize,
) -> Streaming {
    let mut seen = 0;
    let stream = Stream {
        bind: String::new(),
        from,
        pin,
        enough: Box::new(move |_| {
            seen += 1;
            seen > files
        }),
    };
    search_stream(o.clone(), filter, &|| false, stream).expect("stream")
}

fn page(o: &TextSearchOptions, from: u32, pin: Option<Pin>, files: usize) -> Streamed {
    match page_with(o, Arc::new(AllowAll), from, pin, files) {
        Streaming::Streamed(streamed) => *streamed,
        Streaming::Stale => panic!("the pin of a fresh tree went stale"),
    }
}

pub(super) fn paths(streamed: &Streamed) -> Vec<String> {
    streamed
        .result
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect()
}

/// A tree whose hits are spread over directories, with files that hold none
/// between them.
fn tree(count: usize) -> TmpDir {
    let t = TmpDir::new();
    for i in 0..count {
        let hit = if i % 3 == 2 { "none\n" } else { "needle\n" };
        t.write(&format!("d{}/f{i:03}.txt", i % 7), hit);
    }
    t.write("top.txt", "needle\nneedle\n");
    t
}

#[test]
fn a_stream_stops_reading_once_its_page_is_committed() {
    let t = TmpDir::new();
    for i in 0..1000 {
        t.write(&format!("f{i:04}.txt"), "needle\n");
    }
    let o = traversal(&t, "needle");
    let whole = search(o.clone()).expect("whole");
    let first = whole
        .files
        .iter()
        .take(10)
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let cores = crate::cpu::host().cores();
    for busy in [0, cores] {
        // With every core busy the stream takes one worker.
        let guards = (0..busy)
            .map(|_| crate::cpu::host().enter())
            .collect::<Vec<_>>();
        let workers = crate::cpu::host().share();
        let counting = Arc::new(Counting::default());
        let Streaming::Streamed(streamed) = page_with(&o, counting.clone(), 0, None, 10) else {
            panic!("stale");
        };
        drop(guards);
        let read = counting.0.load(Ordering::Relaxed) as usize;
        // The page, the file that overflows it, and what workers held.
        assert!(
            read <= 11 + 2 * workers,
            "{workers} workers read {read} files"
        );
        if workers == 1 {
            assert!(read <= 13, "one worker read {read} files");
        }
        assert_eq!(paths(&streamed)[..10], first[..], "first ten in walk order");
        assert_eq!(streamed.result.files.len(), 11);
        assert!(!streamed.ended);
        assert_eq!(streamed.through, 11);
    }
}

#[test]
fn resumed_streams_concatenate_to_the_whole_traversal_scan_exactly_once() {
    let t = tree(60);
    let o = traversal(&t, "needle");
    let whole = search(o.clone()).expect("whole");
    let expected = whole
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    for size in 1..=7 {
        let mut shown = Vec::new();
        let (mut from, mut pin) = (0, None);
        let mut pages = 0;
        loop {
            let streamed = page(&o, from, pin.take(), size);
            pages += 1;
            let all = paths(&streamed);
            if streamed.ended {
                shown.extend(all);
                break;
            }
            // The file that overflowed opens the next page.
            let overflow = streamed.landmarks.last().expect("an overflowing file");
            shown.extend(all[..all.len() - 1].iter().cloned());
            from = overflow.position;
            pin = Some(Pin {
                covers: from,
                hex: overflow.before.clone(),
            });
            assert!(pages <= expected.len() + 1, "no progress");
        }
        assert_eq!(shown, expected, "page size {size}");
    }
}

#[test]
fn a_page_that_reopens_its_last_file_resumes_inside_it() {
    let t = tree(30);
    let o = traversal(&t, "needle");
    let first = page(&o, 0, None, 3);
    let last = first.landmarks.last().expect("landmark");
    // The cursor stands inside the last file: it is walked again, and the
    // pin covers it.
    let again = page(
        &o,
        last.position,
        Some(Pin {
            covers: last.position + 1,
            hex: last.after.clone(),
        }),
        3,
    );
    assert_eq!(
        paths(&again)[0],
        first.result.files.last().expect("file").path
    );
    assert_eq!(again.landmarks[0].position, last.position);
}

#[test]
fn a_changed_prefix_fails_the_pin() {
    let t = tree(30);
    let o = traversal(&t, "needle");
    let first = page(&o, 0, None, 4);
    let overflow = first.landmarks.last().expect("landmark");
    let pin = || {
        Some(Pin {
            covers: overflow.position,
            hex: overflow.before.clone(),
        })
    };
    // Unchanged: the continuation reads on.
    assert!(matches!(
        page_with(&o, Arc::new(AllowAll), overflow.position, pin(), 4),
        Streaming::Streamed(_)
    ));
    // A file after the cut changes: it is not in the pin.
    t.write("d6/f027.txt", "needle after the cut\n");
    assert!(matches!(
        page_with(&o, Arc::new(AllowAll), overflow.position, pin(), 4),
        Streaming::Streamed(_)
    ));
    // A file before it changes, is added or is removed.
    let before = paths(&first)[0].clone();
    let stamp = stamp_of(Path::new(&before));
    fs::write(&before, "needle plus a longer line than before\n").expect("rewrite");
    assert_ne!(stamp, stamp_of(Path::new(&before)));
    assert!(matches!(
        page_with(&o, Arc::new(AllowAll), overflow.position, pin(), 4),
        Streaming::Stale
    ));
    fs::write(&before, "needle\n").expect("restore");
    t.write("a-new-first-file.txt", "needle\n");
    assert!(matches!(
        page_with(&o, Arc::new(AllowAll), overflow.position, pin(), 4),
        Streaming::Stale
    ));
}

#[test]
fn the_same_cut_comes_back_whatever_the_workers_do() {
    let t = tree(80);
    let o = traversal(&t, "needle");
    let first = page(&o, 0, None, 5);
    let cut = |streamed: &Streamed| (paths(streamed), streamed.through, streamed.pin.clone());
    let expected = cut(&first);
    for _ in 0..20 {
        assert_eq!(cut(&page(&o, 0, None, 5)), expected);
    }
}

/// Counts the directories the walk looked at.
#[derive(Default)]
struct Directories(AtomicU32);

impl TextSearchPathFilter for Directories {
    fn allows(&self, _: &Path, is_dir: bool) -> bool {
        if is_dir {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        true
    }
}

#[test]
fn page_one_lists_no_directory_past_its_cut() {
    let t = TmpDir::new();
    for dir in 0..200 {
        t.write(&format!("d{dir:03}/f.txt"), "needle\n");
    }
    let o = traversal(&t, "needle");
    let directories = Arc::new(Directories::default());
    let Streaming::Streamed(streamed) = page_with(&o, directories.clone(), 0, None, 3) else {
        panic!("stale");
    };
    assert_eq!(streamed.result.files.len(), 4, "the page and its overflow");
    let looked = directories.0.load(Ordering::Relaxed) as usize;
    // The directories of the files read (plus the root and what workers held
    // in flight), never the 200 the tree holds.
    assert!(
        looked < 4 + 2 * crate::cpu::host().share().max(1) + 2,
        "{looked} directories"
    );
}

/// Records the threads that ask the walk's path filter about a file.
#[derive(Default)]
struct WalkingThreads(std::sync::Mutex<std::collections::HashSet<std::thread::ThreadId>>);

impl TextSearchPathFilter for WalkingThreads {
    fn allows(&self, _: &Path, _: bool) -> bool {
        self.0
            .lock()
            .expect("threads")
            .insert(std::thread::current().id());
        true
    }
}

/// One thread walks the tree and feeds the workers, who never take a lock
/// to walk: every walk decision comes from the same thread.
#[test]
fn one_walker_thread_feeds_the_workers() {
    let t = TmpDir::new();
    for i in 0..400 {
        t.write(&format!("d{}/f{i:04}.txt", i % 9), "needle\n");
    }
    let o = traversal(&t, "needle");
    let walking = Arc::new(WalkingThreads::default());
    let Streaming::Streamed(streamed) = page_with(&o, walking.clone(), 0, None, 1000) else {
        panic!("stale");
    };
    assert!(streamed.ended);
    assert_eq!(streamed.result.files.len(), 400);
    let threads = walking.0.lock().expect("threads").len();
    assert_eq!(threads, 1, "the walk was taken by {threads} threads");
}

/// A continuation's prefix is stamped on several threads and folded in walk
/// order: the digest, the digest before the page's first candidate and the
/// opening candidate are those of one thread stamping in order, though the
/// stamps finish out of order.
#[test]
fn a_replayed_prefix_stamped_in_parallel_folds_in_walk_order() {
    use crate::search::text_search::stream::replay_prefix;
    let paths = (0..500)
        .map(|index| std::path::PathBuf::from(format!("/r/d{}/f{index:04}", index % 7)))
        .collect::<Vec<_>>();
    let threads = std::sync::Mutex::new(std::collections::HashSet::new());
    let stamp = |path: &Path| {
        threads
            .lock()
            .expect("threads")
            .insert(std::thread::current().id());
        // Later files finish sooner.
        let name = path.to_string_lossy();
        let last = name.bytes().last().unwrap_or(0);
        std::thread::sleep(std::time::Duration::from_micros(
            u64::from(9 - last % 10) * 50,
        ));
        None
    };
    let replay = |workers| {
        replay_prefix(
            paths.iter().cloned().enumerate(),
            (|path: &std::path::PathBuf| path.as_path(), &stamp),
            (123, paths.len(), workers),
            ListedFiles::default(),
        )
    };
    let serial = replay(1);
    assert_eq!(threads.lock().expect("threads").len(), 1);
    threads.lock().expect("threads").clear();
    let workers = 8;
    for _ in 0..5 {
        let parallel = replay(workers);
        assert_eq!(parallel.count, 500);
        assert_eq!(parallel.check.hex(), serial.check.hex());
        assert_eq!(parallel.at_from.hex(), serial.at_from.hex());
        assert_eq!(
            parallel
                .opening
                .as_ref()
                .map(|(index, path, _)| (*index, path)),
            Some((499, &paths[499])),
        );
    }
    assert!(
        threads.lock().expect("threads").len() > 1,
        "one thread stamped"
    );
}
