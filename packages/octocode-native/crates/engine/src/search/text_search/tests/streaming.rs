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
/// is read too. A first page, or the second one (its page before began at
/// the walk's start).
pub(super) fn page_with(
    o: &TextSearchOptions,
    filter: Arc<dyn TextSearchPathFilter>,
    from: u32,
    pin: Option<Pin>,
    files: usize,
) -> Streaming {
    page_after(o, filter, (from, pin, Anchor::START), files)
}

/// [`page_with`] of a page whose page before began at `anchor`.
pub(super) fn page_after(
    o: &TextSearchOptions,
    filter: Arc<dyn TextSearchPathFilter>,
    (from, pin, anchor): (u32, Option<Pin>, Anchor),
    files: usize,
) -> Streaming {
    let mut seen = 0;
    let stream = Stream {
        bind: String::new(),
        from,
        pin,
        anchor,
        enough: Box::new(move |_| {
            seen += 1;
            seen > files
        }),
    };
    search_stream(o.clone(), filter, &|| false, stream).expect("stream")
}

fn page(o: &TextSearchOptions, from: u32, pin: Option<Pin>, files: usize) -> Streamed {
    page_at(o, (from, pin, Anchor::START), files)
}

fn page_at(o: &TextSearchOptions, at: (u32, Option<Pin>, Anchor), files: usize) -> Streamed {
    match page_after(o, Arc::new(AllowAll), at, files) {
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
        let (mut from, mut pin, mut anchor) = (0, None, Anchor::START);
        let mut pages = 0;
        loop {
            let streamed = page_at(&o, (from, pin.take(), anchor.clone()), size);
            anchor = streamed.anchor.clone();
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
    // The stream's share is pinned, so the bound is the one its walk ran
    // with: the host's share moves with every other call this process
    // computes meanwhile.
    for share in [1, 2, 4, 12] {
        for _ in 0..5 {
            let directories = Arc::new(Directories::default());
            let Streaming::Streamed(streamed) = super::stream::tests::with_share(share, || {
                page_with(&o, directories.clone(), 0, None, 3)
            }) else {
                panic!("stale");
            };
            assert_eq!(streamed.result.files.len(), 4, "the page and its overflow");
            let looked = directories.0.load(Ordering::Relaxed) as usize;
            // The directories of the files read (plus the root and what
            // workers held in flight), never the 200 the tree holds.
            assert!(
                looked < 4 + 2 * share + 2,
                "{looked} directories at share {share}"
            );
        }
    }
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

/// The page before a continuation is stamped on several threads and folded
/// in walk order from that page's first candidate: the digest and the
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
    // The page before began at candidate 40.
    let first = 40;
    let replay = |workers| {
        replay_prefix(
            paths
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, path)| (first + index, path)),
            (|path: &std::path::PathBuf| path.as_path(), &stamp),
            (first, first + 123, first + paths.len(), workers),
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
        assert_eq!(
            parallel
                .opening
                .as_ref()
                .map(|(index, path, _)| (*index, path)),
            Some((first + 499, &paths[499])),
        );
    }
    assert!(
        threads.lock().expect("threads").len() > 1,
        "one thread stamped"
    );
}

/// Records the dot entries the walk reports as left out.
#[derive(Default)]
struct HiddenSkips(std::sync::Mutex<Vec<String>>);

impl TextSearchPathFilter for HiddenSkips {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
    fn skipped_hidden(&self, path: &Path, _: bool) {
        let dir = path.parent().and_then(Path::file_name).unwrap_or_default();
        let name = Path::new(dir).join(path.file_name().unwrap_or_default());
        self.0
            .lock()
            .expect("skips")
            .push(name.to_string_lossy().into_owned());
    }
}

/// The entries a page reports as left out are those before its cut in walk
/// order, whatever the read-ahead ran: never one past it, the same set on
/// every run and for every share of cores.
#[test]
fn skips_are_reported_only_up_to_the_commit_cut_whatever_the_workers_do() {
    let t = TmpDir::new();
    for dir in 0..120 {
        t.write(&format!("d{dir:03}/f.txt"), "needle\n");
        t.write(&format!("d{dir:03}/.h"), "needle\n");
    }
    let o = traversal(&t, "needle");
    let run = |share: usize| {
        let skips = Arc::new(HiddenSkips::default());
        let Streaming::Streamed(streamed) =
            super::stream::tests::with_share(share, || page_with(&o, skips.clone(), 0, None, 2))
        else {
            panic!("stale");
        };
        let mut seen = skips.0.lock().expect("skips").clone();
        seen.sort();
        (streamed.through, seen)
    };
    let (through, expected) = run(1);
    // Three files committed; the dot entry of a directory sorts before its
    // file, so the entries before the last committed file are the three
    // directories' own. The one before the next, uncommitted file is the
    // next page's.
    assert_eq!(through, 3);
    let wanted = (0..3).map(|d| format!("d{d:03}/.h")).collect::<Vec<_>>();
    assert_eq!(expected, wanted);
    for share in [1, 2, 8, 32] {
        for _ in 0..10 {
            assert_eq!(run(share), (through, expected.clone()), "share {share}");
        }
    }
}

/// Walks every page of `o` from the first, `files` files a page, each under
/// `filter_of()` and `share` cores: the files shown in order, and the most
/// files one page's walk decided.
fn walk_pages<F: TextSearchPathFilter + 'static>(
    o: &TextSearchOptions,
    files: usize,
    share: usize,
    filter_of: impl Fn() -> Arc<F>,
    decided: impl Fn(&F) -> usize,
) -> (Vec<String>, usize) {
    let (mut from, mut pin, mut anchor) = (0, None, Anchor::START);
    let (mut shown, mut most) = (Vec::new(), 0);
    loop {
        let filter = filter_of();
        let Streaming::Streamed(streamed) = super::stream::tests::with_share(share, || {
            page_after(o, filter.clone(), (from, pin.take(), anchor.clone()), files)
        }) else {
            panic!("the pin of a fresh tree went stale");
        };
        most = most.max(decided(&filter));
        anchor = streamed.anchor.clone();
        let all = paths(&streamed);
        if streamed.ended {
            shown.extend(all);
            return (shown, most);
        }
        let overflow = streamed.landmarks.last().expect("an overflowing file");
        shown.extend(all[..all.len() - 1].iter().cloned());
        from = overflow.position;
        pin = Some(Pin {
            covers: from,
            hex: overflow.before.clone(),
        });
    }
}

/// A page deep in a walk starts where the page before it began, not at the
/// root: it decides about two pages' worth of files wherever it stands, and
/// the pages still concatenate to the whole scan exactly once.
#[test]
fn a_deep_page_walks_from_the_page_before_not_from_the_root() {
    let t = TmpDir::new();
    for i in 0..1200 {
        t.write(&format!("d{:02}/f{i:04}.txt", i / 20), "needle\n");
    }
    let o = traversal(&t, "needle");
    let whole = search(o.clone())
        .expect("whole")
        .files
        .into_iter()
        .map(|file| file.path)
        .collect::<Vec<_>>();
    let files = 10;
    for share in [1, 4] {
        let (shown, most) = walk_pages(
            &o,
            files,
            share,
            || Arc::new(Counting::default()),
            |counting| counting.0.load(Ordering::Relaxed) as usize,
        );
        assert_eq!(shown, whole, "{share} cores");
        // The page before, this page with the file that overflows it, and
        // what the workers held past it.
        assert!(
            most <= 4 * (files + 1) + 4 * share,
            "{share} cores: a page decided {most} of {} files",
            whole.len()
        );
    }
}

/// A continuation replays the page before it: a file of that page changed,
/// added or removed restarts the walk, wherever the page stands.
#[test]
fn a_change_in_the_page_before_fails_the_pin_on_any_page() {
    let t = tree(60);
    let o = traversal(&t, "needle");
    let first = page(&o, 0, None, 4);
    let cut = first.landmarks.last().expect("landmark");
    let second_at = (
        cut.position,
        Some(Pin {
            covers: cut.position,
            hex: cut.before.clone(),
        }),
        first.anchor.clone(),
    );
    let second = page_at(&o, second_at, 4);
    let cut = second.landmarks.last().expect("landmark");
    let third = || {
        page_after(
            &o,
            Arc::new(AllowAll),
            (
                cut.position,
                Some(Pin {
                    covers: cut.position,
                    hex: cut.before.clone(),
                }),
                second.anchor.clone(),
            ),
            4,
        )
    };
    assert!(matches!(third(), Streaming::Streamed(_)));
    // A file the second page showed changes.
    let shown = paths(&second)[1].clone();
    let kept = fs::read_to_string(&shown).expect("read");
    fs::write(&shown, "needle plus a longer line than before\n").expect("rewrite");
    assert!(matches!(third(), Streaming::Stale));
    fs::remove_file(&shown).expect("remove");
    assert!(matches!(third(), Streaming::Stale));
    // Restored as it was (a new stamp): still not what the page read.
    fs::write(&shown, kept).expect("restore");
    assert!(matches!(third(), Streaming::Stale));
}

/// Collects the dot entries every page reports as left out.
#[derive(Default)]
struct DotSkips(std::sync::Mutex<Vec<String>>);

impl TextSearchPathFilter for DotSkips {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
    fn skipped_hidden(&self, path: &Path, _: bool) {
        self.0
            .lock()
            .expect("skips")
            .push(path.to_string_lossy().into_owned());
    }
}

/// Each entry the walk leaves out is reported once across a walk's pages,
/// by the page whose stretch of the walk holds it: the pages together report
/// what one whole walk reports.
#[test]
fn entries_left_out_are_reported_once_across_pages() {
    let t = TmpDir::new();
    for dir in 0..12 {
        for file in 0..3 {
            t.write(&format!("d{dir:02}/f{file}.txt"), "needle\n");
            t.write(&format!("d{dir:02}/.h{file}"), "needle\n");
        }
    }
    let o = traversal(&t, "needle");
    let whole = Arc::new(DotSkips::default());
    let Streaming::Streamed(_) = page_with(&o, whole.clone(), 0, None, 10_000) else {
        panic!("stale");
    };
    let mut expected = whole.0.lock().expect("skips").clone();
    expected.sort();
    assert_eq!(expected.len(), 36);
    for files in [1, 2, 5] {
        let reported = Arc::new(DotSkips::default());
        walk_pages(&o, files, 4, || Arc::clone(&reported), |_| 0);
        let mut seen = reported.0.lock().expect("skips").clone();
        seen.sort();
        assert_eq!(seen, expected, "pages of {files}");
    }
}
