//! A replayed walk is the walk it records, for every caller and cut; a tree
//! share parses each source once for the scans taking part, and a scan of
//! its own gives the same result.
use super::super::walk::collect_files;
use super::*;
use crate::search::walk::{GlobalIgnore, Prune};
use crate::structural::types::{StructuralDemand, StructuralSearchFilesOptions};
use crate::test_support::TmpDir;
use std::fs;

/// A scope with nested directories, a link, an ignored file and directory,
/// a dot directory and a default-pruned one.
fn scope() -> TmpDir {
    let root = TmpDir::new();
    for (path, text) in [
        ("a.rs", "fn a() { x.push(1); }\n"),
        ("b.ts", "push(b);\n"),
        ("src/c.rs", "fn c() { y.push(2); y.clone(); }\n"),
        ("src/deep/d.rs", "fn d() { z.clone(); }\n"),
        ("src/notes.md", "push\n"),
        ("skip.log", "push\n"),
        ("ignored/e.rs", "fn e() {}\n"),
        (".hidden/f.rs", "fn f() {}\n"),
        ("node_modules/g.rs", "fn g() {}\n"),
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("parent")).expect("dir");
        fs::write(file, text).expect("file");
    }
    fs::write(root.join(".ignore"), "*.log\nignored/\n").expect("ignore file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("a.rs"), root.join("link.rs")).expect("link");
    root
}

/// What a walk gives: its candidates, errors and left-out entries.
type Walked = (
    Vec<PathBuf>,
    crate::structural::types::WalkErrors,
    crate::structural::types::WalkSkipped,
);

/// One walk of `root` cut at `max_files`, admitting by `admits`, and every
/// `admit` call it made (path, and whether a directory).
fn walked(
    root: &Path,
    max_files: usize,
    walk: Walk<'_>,
    admits: &(dyn Fn(&Path) -> bool + Sync),
) -> (Walked, Vec<(PathBuf, bool)>) {
    let globs = crate::search::glob::compile(root, &[], &[]).expect("globs");
    let calls = Mutex::new(Vec::new());
    let admit = |path: &Path, kind: Option<std::fs::FileType>| {
        calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((path.to_path_buf(), kind.is_some_and(|kind| kind.is_dir())));
        admits(path)
    };
    let out = collect_files(
        root,
        &super::super::walk::Walked {
            globs: &globs,
            defaults: Some(Prune::new(["node_modules"]).at_with(root, GlobalIgnore::NONE)),
            ignore: crate::types::IgnorePolicy::default(),
            global_ignore: GlobalIgnore::NONE,
            max_depth: None,
            max_files,
        },
        (&admit, walk),
        &|| false,
    )
    .expect("walk");
    (
        out,
        calls.into_inner().unwrap_or_else(PoisonError::into_inner),
    )
}

#[test]
fn a_replayed_walk_admits_and_records_what_a_walk_does_at_every_cut() {
    let root = scope();
    let recorded = Mutex::new(None);
    let keep = |record: Option<WalkRecord>| {
        *recorded.lock().unwrap_or_else(PoisonError::into_inner) = record;
    };
    walked(&root, usize::MAX, Walk::Record(&keep), &|_| true);
    let record = recorded
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .expect("a whole walk is recorded");
    assert_eq!(
        record.dirs().first().map(PathBuf::as_path),
        Some(root.as_ref())
    );
    assert!(record.dirs().contains(&root.join("src/deep")));
    // Callers that admit differently, each at every cut.
    let only_rust = |path: &Path| path.extension().is_some_and(|ext| ext == "rs");
    let callers: [&(dyn Fn(&Path) -> bool + Sync); 3] = [&|_| true, &only_rust, &|_| false];
    for admits in callers {
        for max_files in [0, 1, 2, 3, 5, usize::MAX] {
            let fresh = walked(&root, max_files, Walk::Fresh, admits);
            let replayed = walked(&root, max_files, Walk::Replay(&record), admits);
            assert_eq!(replayed, fresh, "cut at {max_files}");
        }
    }
    let (whole, _) = walked(&root, usize::MAX, Walk::Fresh, &|_| true);
    assert!(
        !whole.2.is_empty(),
        "the walk leaves entries out: {:?}",
        whole.2
    );
}

#[test]
fn a_walk_cut_before_its_end_records_nothing() {
    let root = scope();
    let recorded = Mutex::new(Some(None));
    let keep = |record: Option<WalkRecord>| {
        *recorded.lock().unwrap_or_else(PoisonError::into_inner) = Some(record);
    };
    walked(&root, 1, Walk::Record(&keep), &|_| true);
    assert!(matches!(
        recorded
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner),
        Some(None)
    ));
}

fn parse(text: &str) -> Result<tree_sitter::Tree, ()> {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).expect("grammar");
    parser.parse(text, None).ok_or(())
}

#[test]
fn a_share_parses_each_text_once_for_the_scans_taking_part() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let share = TreeShare::new();
    let path = Path::new("/scope/a.rs");
    // A lone scan keeps nothing.
    let alone = share.join();
    share
        .tree((path, &language, "fn a() {}"), &|| false, || {
            parse("fn a() {}")
        })
        .expect("tree");
    share
        .tree((path, &language, "fn a() {}"), &|| false, || {
            parse("fn a() {}")
        })
        .expect("tree");
    assert_eq!((share.parses(), share.kept()), (2, 0));
    let other = share.join();
    let first = share
        .tree((path, &language, "fn a() {}"), &|| false, || {
            parse("fn a() {}")
        })
        .expect("tree");
    let again = share
        .tree((path, &language, "fn a() {}"), &|| false, || {
            parse("fn a() {}")
        })
        .expect("tree");
    assert_eq!((share.parses(), share.kept()), (3, 1));
    assert_eq!(first.root_node().to_sexp(), again.root_node().to_sexp());
    // Another text of the same path is parsed again.
    let changed = share
        .tree((path, &language, "fn b() {}"), &|| false, || {
            parse("fn b() {}")
        })
        .expect("tree");
    assert_eq!(share.parses(), 4);
    assert!(changed.root_node().to_sexp().contains("function_item"));
    drop((alone, other));
}

#[test]
fn a_scan_waits_for_the_parse_another_makes_of_the_same_text() {
    let language = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
    let share = TreeShare::new();
    let path = Path::new("/scope/a.rs");
    let (_one, _two) = (share.join(), share.join());
    let started = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let leader = scope.spawn(|| {
            share.tree((path, &language, "fn a() {}"), &|| false, || {
                started.wait();
                // The other scan asks while this parse is under way.
                while Arc::strong_count(
                    share
                        .trees
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .values()
                        .next()
                        .expect("under way"),
                ) < 3
                {
                    std::thread::yield_now();
                }
                parse("fn a() {}")
            })
        });
        started.wait();
        let waited = share
            .tree((path, &language, "fn a() {}"), &|| false, || {
                parse("fn a() {}")
            })
            .expect("tree");
        let led = leader.join().expect("leader").expect("tree");
        assert_eq!(waited.root_node().to_sexp(), led.root_node().to_sexp());
    });
    assert_eq!(share.parses(), 1);
}

/// Different rules over one scope, sharing a recorded walk and a tree
/// share, give what each gives alone, and parse each source once.
#[test]
fn rules_sharing_a_walk_and_trees_match_as_scans_of_their_own() {
    let root = scope();
    let options = |pattern: &str| StructuralSearchFilesOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: Some(pattern.to_owned()),
        rule: None,
        include: None,
        exclude: None,
        default_exclude_dir: Some(vec!["node_modules".to_owned()]),
        ignore: crate::types::IgnorePolicy::default(),
        max_depth: None,
        max_files: Some(50),
        skip_files: None,
        max_file_bytes: None,
        judge: None,
        global_ignore: GlobalIgnore::NONE,
        kept: None,
    };
    let extension = |path: &Path| crate::text::extension_of(&path.to_string_lossy(), true, "");
    let lines = |result: &crate::structural::types::StructuralSearchFilesDetailedResult| {
        result
            .files
            .iter()
            .flat_map(|file| {
                file.matches
                    .held()
                    .iter()
                    .map(|m| (file.path.clone(), m.start_line, m.text.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let alone = |pattern: &str| {
        crate::structural::files::search_files(
            options(pattern),
            StructuralDemand::Matches,
            &|_, _| true,
            &extension,
            &|| false,
            None,
        )
        .expect("scan")
    };
    let recorded = Mutex::new(None);
    let keep = |record: Option<WalkRecord>| {
        *recorded.lock().unwrap_or_else(PoisonError::into_inner) = record;
    };
    let share = TreeShare::new();
    let (_one, _two) = (share.join(), share.join());
    let shared = |pattern: &str, walk: Walk<'_>| {
        crate::structural::files::search_files_shared(
            options(pattern),
            StructuralDemand::Matches,
            (&|_, _| true, None),
            &extension,
            &|| false,
            crate::structural::files::ScanShare {
                walk,
                trees: Some(&share),
            },
        )
        .expect("scan")
    };
    let first = shared("$X.push($Y)", Walk::Record(&keep));
    let record = recorded
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .expect("recorded");
    let parsed = share.parses();
    let second = shared("$X.push(1)", Walk::Replay(&record));
    assert_eq!(lines(&first), lines(&alone("$X.push($Y)")));
    assert_eq!(lines(&second), lines(&alone("$X.push(1)")));
    assert_eq!(second.parsed_files, alone("$X.push(1)").parsed_files);
    assert!(!lines(&second).is_empty());
    // The second rule reads only sources the first parsed: it parses none.
    assert_eq!(share.parses(), parsed);
}
