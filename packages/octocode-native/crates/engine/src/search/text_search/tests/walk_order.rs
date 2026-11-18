//! The stream's own walk: the entries and the order the `ignore` walker
//! sorted by name gives, the same candidate index for every path whatever
//! the threads do, and no directory read that the rules leave out.

use super::streaming::{page_after, page_with, paths, traversal};
use super::*;
use crate::search::text_search::stream::tests::with_share;
use crate::search::walk::read_ahead::{listed_under, queued_under};
use crate::types::IgnorePolicy;
use ignore::overrides::Override;
use std::ffi::OsStr;
use std::path::PathBuf;

/// Every entry the walk's path filter was asked about, in the order it was
/// asked: the entries no rule left out, as the walk reached them.
#[derive(Default)]
struct Entries(Mutex<Vec<PathBuf>>);

impl TextSearchPathFilter for Entries {
    fn allows(&self, path: &Path, _: bool) -> bool {
        self.0.lock().expect("lock").push(path.to_path_buf());
        true
    }
}

/// A repeatable pseudo-random choice: no dependency, the same tree per seed.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % n
    }
}

const NAMES: [&str; 14] = [
    "a",
    "a-b",
    "a.c",
    "B",
    "b",
    "_u",
    "z9",
    "é",
    "src",
    "build",
    "gen",
    "node_modules",
    ".hid",
    "sub1",
];
const FILES: [&str; 9] = [
    "f1.rs", "x.log", "keep.log", "note.txt", "a.md", ".dot.rs", "a.txt", "Zed.rs", "a-b.rs",
];
const GITIGNORES: [&str; 6] = [
    "*.log\nbuild/\n!keep.log\n",
    "/gen\nsub*/\n",
    "node_modules\n*.txt\n!a.txt\n",
    "a.c\n.hid\n",
    "!x.log\n",
    "src/*.rs\n",
];
const IGNORES: [&str; 3] = ["!keep.log\n", "*.md\n", "b\n"];

/// A tree of nested directories with names that sort differently as paths
/// and as names (`a`, `a-b`, `a.c`), dot entries, and ignore files of every
/// kind at every depth, inside a repository.
fn generated(seed: u64) -> TmpDir {
    let tree = TmpDir::new();
    tree.write(".git/HEAD", "ref: refs/heads/main\n");
    let mut dice = Lcg(seed);
    fill(&tree, "", 0, &mut dice);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("a", tree.0.join("link-dir")).ok();
        std::os::unix::fs::symlink("f1.rs", tree.0.join("link-file")).ok();
    }
    tree
}

/// Write `content` at `relative` unless a file already stands where a
/// directory is wanted (or the reverse): the generated names may meet.
fn put(tree: &TmpDir, relative: &str, content: &str) {
    let path = tree.0.join(relative);
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    if !path.is_dir() {
        std::fs::write(path, content).ok();
    }
}

fn fill(tree: &TmpDir, dir: &str, depth: usize, dice: &mut Lcg) {
    let at = |name: &str| {
        if dir.is_empty() {
            name.to_owned()
        } else {
            format!("{dir}/{name}")
        }
    };
    if dice.below(3) == 0 {
        put(
            tree,
            &at(".gitignore"),
            GITIGNORES[dice.below(GITIGNORES.len())],
        );
    }
    if dice.below(4) == 0 {
        put(tree, &at(".ignore"), IGNORES[dice.below(IGNORES.len())]);
    }
    let mut taken = std::collections::BTreeSet::new();
    for _ in 0..4 + dice.below(5) {
        let name = if dice.below(5) < 3 {
            NAMES[dice.below(NAMES.len())]
        } else {
            FILES[dice.below(FILES.len())]
        };
        if !taken.insert(name) {
            continue;
        }
        let is_dir = NAMES.contains(&name) && depth < 4 && dice.below(5) != 0;
        if is_dir {
            fill(tree, &at(name), depth + 1, dice);
            put(tree, &at(&format!("{name}/inner.rs")), "x\n");
        } else if !NAMES.contains(&name) || !is_dir {
            // A name of the directory pool is a plain file where it is not
            // a directory.
            if !tree.0.join(at(name)).is_dir() {
                put(tree, &at(name), "x\n");
            }
        }
    }
}

/// How many directories lie under `root`.
fn walkdir_count(root: &Path) -> usize {
    std::fs::read_dir(root)
        .map(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .map(|entry| 1 + walkdir_count(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

/// The entries the `ignore` walker, sorted by file name and applying its own
/// rules, yields below `root` (and the root).
fn ignore_walk(
    root: &Path,
    policy: IgnorePolicy,
    include: &Option<Vec<String>>,
    lang: &Option<String>,
) -> Vec<PathBuf> {
    let mut walker = crate::search::walk::walk_builder(root, policy, None);
    walker.sort_by_file_name(OsStr::cmp);
    if let Some(include) = include {
        let overrides: Override = crate::search::glob::compile(root, include, &[])
            .expect("globs")
            .overrides();
        walker.overrides(overrides);
    }
    if let Some(lang) = lang {
        let mut types = TypesBuilder::new();
        types.add_defaults();
        types.select(lang);
        walker.types(types.build().expect("types"));
    }
    walker
        .build()
        .filter_map(std::result::Result::ok)
        .map(ignore::DirEntry::into_path)
        .collect()
}

/// The entries the stream's walk reached below `root`, in order.
fn stream_walk(
    root: &Path,
    policy: IgnorePolicy,
    include: &Option<Vec<String>>,
    lang: &Option<String>,
) -> Vec<PathBuf> {
    let entries = Arc::new(Entries::default());
    let mut options = opts(root.to_string_lossy().into_owned(), "ZZQQnomatchZZ");
    options.sort = SearchSort::Traversal;
    options.ignore = policy;
    options.include.clone_from(include);
    options.lang_type.clone_from(lang);
    let Streaming::Streamed(streamed) = search_stream(
        options,
        entries.clone(),
        &|| false,
        Stream {
            bind: String::new(),
            from: 0,
            pin: None,
            anchor: Anchor::START,
            enough: Box::new(|_| false),
        },
    )
    .expect("stream") else {
        panic!("stale");
    };
    assert!(streamed.ended);
    entries.0.lock().expect("lock").clone()
}

type Scope = (Option<Vec<String>>, Option<String>);

fn scopes() -> [Scope; 3] {
    [
        (None, None),
        (Some(vec!["*.log".into(), "**/*.rs".into()]), None),
        (None, Some("rust".into())),
    ]
}

fn policies() -> [IgnorePolicy; 4] {
    [
        IgnorePolicy::default(),
        IgnorePolicy::new(Some(true), None),
        IgnorePolicy::new(None, Some(true)),
        IgnorePolicy::ALL,
    ]
}

/// The stream's walk reaches the same entries, in the same order, as the
/// `ignore` walker sorted by file name, under every policy and scope, on
/// trees with ignore files of every kind nested at every depth.
#[test]
fn the_walk_is_the_ignore_walk_sorted_by_name_on_generated_trees() {
    let mut read_ahead = 0;
    for seed in 0..12 {
        let tree = generated(seed);
        let dirs = walkdir_count(&tree.0);
        // A tree of more than the directories a walk reads itself, so the
        // workers that read ahead apply the rules too.
        read_ahead +=
            usize::from(dirs > crate::search::walk::read_ahead::START_AFTER_DIRECTORIES as usize);
        let roots = [tree.0.clone(), tree.0.join("a"), tree.0.join("src")];
        for root in roots.iter().filter(|root| root.is_dir()) {
            for policy in policies() {
                for (include, lang) in scopes() {
                    let walked = stream_walk(root, policy, &include, &lang);
                    let expected = ignore_walk(root, policy, &include, &lang);
                    assert_eq!(
                        walked, expected,
                        "seed {seed} {root:?} {policy:?} {include:?} {lang:?}"
                    );
                }
            }
        }
    }
    assert!(read_ahead >= 4, "{read_ahead} of 12 trees read ahead");
}

/// The same on real trees: this workspace's crates, and, when the benchmark
/// checkout is there, a large TypeScript one.
#[test]
fn the_walk_is_the_ignore_walk_sorted_by_name_on_real_trees() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates")
        .to_path_buf();
    let huge = crates.join("../../octocode-benchmark/local/repos/huge-ts");
    for root in [crates, huge] {
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        for policy in [IgnorePolicy::default(), IgnorePolicy::new(Some(true), None)] {
            let walked = stream_walk(&root, policy, &None, &None);
            let expected = ignore_walk(&root, policy, &None, &None);
            assert!(walked.len() > 50, "{root:?}: {} entries", walked.len());
            assert_eq!(walked, expected, "{root:?} {policy:?}");
        }
    }
}

/// A directory the `ignore` walker cannot read is an error whose text is the
/// walker's own, and the walk goes on past it.
#[cfg(unix)]
#[test]
fn an_unreadable_directory_reads_as_the_ignore_walker_reports_it() {
    use std::os::unix::fs::PermissionsExt;
    let tree = TmpDir::new();
    tree.write("a/f.rs", "x\n");
    tree.write("locked/f.rs", "x\n");
    tree.write("z/f.rs", "x\n");
    let locked = tree.0.join("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    if std::fs::read_dir(&locked).is_ok() {
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("restore");
        return;
    }
    let expected = crate::search::walk::walk_builder(&tree.0, IgnorePolicy::ALL, None)
        .build()
        .filter_map(std::result::Result::err)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    let mut options = opts(tree.path(), "x");
    options.sort = SearchSort::Traversal;
    let found = page_with(&options, Arc::new(AllowAll), 0, None, 100);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("restore");
    let Streaming::Streamed(streamed) = found else {
        panic!("stale")
    };
    assert_eq!(expected.len(), 1, "{expected:?}");
    assert_eq!(streamed.result.stats.error_count, Some(1));
    assert_eq!(
        streamed.result.stats.first_error.as_deref(),
        Some(expected[0].as_str())
    );
    assert_eq!(streamed.result.files.len(), 2, "the walk went on");
}

/// A tree wide and deep enough that the walk reads ahead: many directories,
/// each holding a hit, and a gitignored `node_modules` (and a dot directory)
/// at several levels.
fn wide() -> TmpDir {
    let tree = TmpDir::new();
    tree.write(".git/HEAD", "ref: refs/heads/main\n");
    tree.write(".gitignore", "node_modules\n");
    for dir in 0..40 {
        tree.write(&format!("src/d{dir:02}/f.txt"), "needle\n");
        tree.write(&format!("src/d{dir:02}/inner/g.txt"), "needle\n");
        tree.write(
            &format!("src/d{dir:02}/node_modules/pkg{}/index.txt", dir % 3),
            "needle\n",
        );
        tree.write(&format!("src/d{dir:02}/.cache/c.txt"), "needle\n");
    }
    for pkg in 0..8 {
        tree.write(&format!("node_modules/p{pkg}/i.txt"), "needle\n");
    }
    tree
}

/// The directories the rules leave out are never listed, by the walk or by
/// the threads that read ahead of it, and never queued to them; the ones
/// walked are (the control: the walk lists through the same reader).
#[test]
fn no_directory_the_rules_leave_out_is_ever_listed() {
    let tree = wide();
    let root = tree.0.canonicalize().expect("root");
    let o = traversal(&tree, "needle");
    let streamed = with_share(8, || page_with(&o, Arc::new(AllowAll), 0, None, 10_000));
    let Streaming::Streamed(streamed) = streamed else {
        panic!("stale")
    };
    assert!(streamed.ended);
    assert_eq!(streamed.result.files.len(), 80, "{:?}", paths(&streamed));
    let listed = listed_under(&tree.0);
    let queued = queued_under(&tree.0);
    assert!(
        listed.iter().any(|dir| dir.ends_with("src/d07/inner")),
        "the control: walked directories are listed: {} listed under {root:?}",
        listed.len()
    );
    assert!(
        !queued.is_empty(),
        "the walk of 80 directories read some ahead"
    );
    for dir in listed.iter().chain(&queued) {
        let shown = dir.to_string_lossy();
        assert!(
            !shown.contains("node_modules") && !shown.contains(".cache"),
            "listed or queued {shown}"
        );
    }
}

/// The candidate index of a path is the same whatever the threads do: the
/// pages a stream cuts, their pins and landmarks do not depend on how many
/// threads search or read ahead, and a pin from one run resumes under any
/// other.
#[test]
fn candidate_indexes_and_pins_hold_under_any_number_of_threads() {
    let tree = wide();
    let o = traversal(&tree, "needle");
    let page_from = |share: usize, at: (u32, Option<Pin>, Anchor), files: usize| {
        let Streaming::Streamed(streamed) =
            with_share(share, || page_after(&o, Arc::new(AllowAll), at, files))
        else {
            panic!("stale")
        };
        *streamed
    };
    let one_page = |share: usize, from: u32, pin: Option<Pin>, files: usize| {
        page_from(share, (from, pin, Anchor::START), files)
    };
    let cut = |streamed: &Streamed| {
        (
            paths(streamed),
            streamed.through,
            streamed.pin.clone(),
            streamed
                .landmarks
                .iter()
                .map(|landmark| {
                    (
                        landmark.position,
                        landmark.before.clone(),
                        landmark.after.clone(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    };
    let whole = one_page(1, 0, None, 10_000);
    let expected = paths(&whole);
    assert_eq!(expected.len(), 80);
    assert!(
        expected
            .windows(2)
            .all(|pair| Path::new(&pair[0]) < Path::new(&pair[1])),
        "{expected:?}"
    );
    let first = cut(&one_page(1, 0, None, 9));
    for share in [1, 2, 4, 12, 32] {
        for _ in 0..4 {
            assert_eq!(cut(&one_page(share, 0, None, 9)), first, "{share} threads");
        }
        assert_eq!(paths(&one_page(share, 0, None, 10_000)), expected);
    }
    // A pin from a run of 32 resumes under a run of 1 and the reverse, and
    // the pages concatenate to the whole scan exactly once.
    for (before, after) in [(32, 1), (1, 32), (12, 3)] {
        let mut shown = Vec::new();
        let (mut from, mut pin, mut anchor) = (0, None, Anchor::START);
        let mut share = before;
        loop {
            let streamed = page_from(share, (from, pin.take(), anchor.clone()), 11);
            anchor = streamed.anchor.clone();
            let all = paths(&streamed);
            if streamed.ended {
                shown.extend(all);
                break;
            }
            let overflow = streamed.landmarks.last().expect("an overflowing file");
            shown.extend(all[..all.len() - 1].iter().cloned());
            from = overflow.position;
            pin = Some(Pin {
                covers: from + 1,
                hex: overflow.after.clone(),
            });
            share = if share == before { after } else { before };
        }
        assert_eq!(shown, expected, "{before} then {after} threads");
    }
}

/// Page one of a deep tree reads the directories of its files and no more:
/// the walk reads ahead only after it has read a few directories itself.
#[test]
fn page_one_reads_no_directory_past_its_cut() {
    let t = TmpDir::new();
    for dir in 0..400 {
        t.write(&format!("d{dir:03}/f.txt"), "needle\n");
    }
    let o = traversal(&t, "needle");
    let Streaming::Streamed(streamed) =
        with_share(12, || page_with(&o, Arc::new(AllowAll), 0, None, 3))
    else {
        panic!("stale")
    };
    assert_eq!(streamed.result.files.len(), 4);
    let listed = listed_under(&t.0);
    // The root, the directories of the page, and those the window of
    // candidates ahead of the cut held; never the 400 the tree holds.
    assert!(
        listed.len() <= 1 + 4 + 2 * 12 + 1,
        "page one listed {} directories: {listed:?}",
        listed.len()
    );
}

/// A walker blocked on the full window ends when the caller cancels: the
/// files handed out before the cut are committed, none hangs.
#[test]
fn a_walker_blocked_on_a_full_window_ends_when_the_caller_cancels() {
    let t = TmpDir::new();
    for index in 0..400 {
        t.write(&format!("d{}/f{index:04}.txt", index % 5), "needle\n");
    }
    let o = traversal(&t, "needle");
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = Arc::new(AtomicU32::new(0));
    let (done, finished) = std::sync::mpsc::channel();
    let handle = {
        let (cancelled, seen) = (cancelled.clone(), seen.clone());
        std::thread::spawn(move || {
            with_share(2, || {
                // Committing a file is slow, so the one worker keeps the
                // window full and the walker waits on it.
                let stream = Stream {
                    bind: String::new(),
                    from: 0,
                    pin: None,
                    anchor: Anchor::START,
                    enough: Box::new(move |_| {
                        seen.fetch_add(1, Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        false
                    }),
                };
                let streamed = search_stream(
                    o,
                    Arc::new(AllowAll),
                    &|| cancelled.load(Ordering::Relaxed),
                    stream,
                );
                done.send(()).ok();
                streamed
            })
        })
    };
    while seen.load(Ordering::Relaxed) < 5 {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cancelled.store(true, Ordering::Relaxed);
    finished
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the cancelled stream did not return");
    let Streaming::Streamed(streamed) = handle.join().expect("stream").expect("result") else {
        panic!("stale")
    };
    assert!(!streamed.ended);
    let committed = streamed.through as usize;
    assert!((5..400).contains(&committed), "{committed} committed");
    assert_eq!(
        streamed.result.files.len(),
        committed,
        "every file handed out before the cut is committed, in order"
    );
}
