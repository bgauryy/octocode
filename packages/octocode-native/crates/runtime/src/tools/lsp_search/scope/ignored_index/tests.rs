use super::*;
use crate::tools::test_support::workspace_policy;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::create_dir_all(root.join(".git")).expect("git");
    (dir, root)
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, text).expect("write");
}

fn scope_of(root: &Path) -> Scope {
    Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()])
}

/// The index lists the ignored family files once; every later request,
/// for any name, reads only those files under the same index, which
/// follows the snapshot: an added ignored file is listed next time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_index_is_built_once_per_snapshot_and_serves_every_name() {
    let (_dir, root) = fixture();
    write(&root.join(".gitignore"), "gen/\n");
    write(
        &root.join("a.ts"),
        "export function f() {}\nexport function g() {}\n",
    );
    write(&root.join("b.ts"), "f();\n");
    for index in 0..50 {
        write(&root.join(format!("gen/g{index}.ts")), "f();\n");
    }
    write(&root.join("gen/h.ts"), "g();\n");
    write(&root.join("gen/notes.md"), "f g\n");
    let policy = workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    let skip = HashSet::new();
    let scope = scope_of(&root);

    let found = scope
        .ignored_text_files("f", &skip, &policy, cancel)
        .await
        .expect("scan");
    assert!(!found.unfinished, "{found:?}");
    assert_eq!(found.files.len(), 50);
    assert_eq!(found.rules, [".gitignore:1 'gen/'"]);
    let index = scope
        .ignored_index(&policy, cancel)
        .await
        .expect("index")
        .expect("built");
    // Only the family files the ignore rules hide are listed.
    assert_eq!(index.files.len(), 51, "{index:?}");
    assert!(index.complete);

    // Another name reads the same index.
    let other = scope
        .ignored_text_files("g", &skip, &policy, cancel)
        .await
        .expect("scan");
    assert_eq!(other.files, [root.join("gen/h.ts").to_string_lossy()]);
    let again = scope
        .ignored_index(&policy, cancel)
        .await
        .expect("index")
        .expect("held");
    assert!(Arc::ptr_eq(&index, &again), "a held index is reused");

    // A new ignored file changes its directory's stamp: rebuilt.
    write(&root.join("gen/new.ts"), "f();\n");
    let changed = scope
        .ignored_text_files("f", &skip, &policy, cancel)
        .await
        .expect("scan");
    assert_eq!(changed.files.len(), 51, "{changed:?}");
    // So does an edited ignore file beside a walked directory.
    write(&root.join(".gitignore"), "gen/g1*.ts\n");
    let narrowed = scope
        .ignored_text_files("f", &skip, &policy, cancel)
        .await
        .expect("scan");
    assert_eq!(narrowed.files.len(), 11, "{narrowed:?}");
}

/// An index whose walk stopped at its bounds lists what it reached and
/// leaves the scan unfinished: never read as "no ignored file".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_incomplete_index_reports_an_unfinished_scan() {
    let (_dir, root) = fixture();
    write(&root.join(".gitignore"), "gen/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    let reached = root.join("gen/g.ts");
    write(&reached, "f();\n");
    let policy = workspace_policy(&root);
    let scope = scope_of(&root);
    INDEXES.insert(
        scope.index_key(&policy),
        IgnoredIndex {
            files: vec![reached.to_string_lossy().into_owned()],
            complete: false,
            ..IgnoredIndex::default()
        },
        1,
        Some(INCOMPLETE_TTL),
    );
    let found = scope
        .ignored_text_files(
            "f",
            &HashSet::new(),
            &policy,
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan");
    assert!(found.unfinished, "{found:?}");
    assert_eq!(found.files, [reached.to_string_lossy()], "{found:?}");
}

/// A request waits for the index a walk is building, however long the walk
/// takes, so requests that arrive while it runs (several at once, under a
/// loaded host) all get the whole answer, never a shorter one; a request
/// that ends while it waits (cancelled, or past its deadline) fails as
/// cancelled instead of answering with an unfinished scan.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn requests_wait_for_a_building_index_until_it_lands_or_they_end() {
    struct Ends(std::sync::atomic::AtomicBool);
    impl CancellationCheck for Ends {
        fn check(&self) -> Result<(), String> {
            if self.0.load(std::sync::atomic::Ordering::SeqCst) {
                Err("deadline".to_owned())
            } else {
                Ok(())
            }
        }
    }
    let (_dir, root) = fixture();
    write(&root.join(".gitignore"), "gen/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    for index in 0..20 {
        write(&root.join(format!("gen/g{index}.ts")), "f();\n");
    }
    let policy = workspace_policy(&root);
    let scope = scope_of(&root);
    let key = scope.index_key(&policy);
    // A walk in progress: the building mark is held while requests wait.
    let mark = || {
        BUILDING
            .lock()
            .expect("lock")
            .get_or_insert_with(HashSet::new)
            .insert(key.clone());
    };
    mark();
    let skip = HashSet::new();
    let ends = Ends(false.into());
    let (ended, ()) = tokio::join!(
        scope.ignored_text_files("f", &skip, &policy, &ends),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            ends.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    );
    assert!(
        matches!(&ended, Err(failure) if failure.code == octocode_contract_types::ErrorCode::Cancelled),
        "a request that ends while it waits fails, not answers short: {ended:?}"
    );
    let cancel = &crate::execution::cancel::NeverCancel;
    let waiting = (0..6).map(|_| scope.ignored_text_files("f", &skip, &policy, cancel));
    let release = async {
        // Busy the host while the requests wait, then let the walk land.
        let load = (0..8)
            .map(|_| tokio::task::spawn_blocking(|| (0..2_000_000_u64).fold(0, u64::wrapping_add)))
            .collect::<Vec<_>>();
        for task in load {
            task.await.expect("load");
        }
        // The walk lands as a background build does: index first, mark last.
        let index = build(&root, &scope.include, &policy, &|| false);
        INDEXES.insert(key.clone(), index, 1, None);
        drop(Building(key.clone()));
    };
    let (answers, ()) = tokio::join!(futures_util::future::join_all(waiting), release);
    for answer in answers {
        let answer = answer.expect("scan");
        assert!(!answer.unfinished, "{answer:?}");
        assert_eq!(answer.files.len(), 20, "{answer:?}");
    }
}

/// Saving a tracked file by rename changes its directory's stamp but adds
/// nothing ignored: the index holds. A new file that ignore rules hide in a
/// tracked directory, or a new directory, stales it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tracked_directory_edit_keeps_the_index_unless_it_hides_a_file() {
    let (_dir, root) = fixture();
    write(&root.join(".gitignore"), "gen/\n*.gen.ts\n");
    write(&root.join("src/a.ts"), "export function f() {}\n");
    write(&root.join("gen/g.ts"), "f();\n");
    let policy = workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    let scope = scope_of(&root);
    let held = || async {
        scope
            .ignored_index(&policy, cancel)
            .await
            .expect("index")
            .expect("built")
    };
    let first = held().await;
    // An editor's save: a temporary file renamed over the original.
    write(
        &root.join("src/a.ts.tmp"),
        "export function f() { return 1; }\n",
    );
    std::fs::rename(root.join("src/a.ts.tmp"), root.join("src/a.ts")).expect("rename");
    let saved = held().await;
    assert!(Arc::ptr_eq(&first, &saved), "an edit keeps the index");
    // A generated file the rules hide, beside tracked ones.
    write(&root.join("src/b.gen.ts"), "f();\n");
    let regenerated = held().await;
    assert!(
        !Arc::ptr_eq(&saved, &regenerated),
        "a hidden file stales it"
    );
    assert!(
        regenerated
            .files
            .iter()
            .any(|file| file.ends_with("src/b.gen.ts")),
        "{regenerated:?}"
    );
    // A new directory may hold anything.
    write(&root.join("src/new/c.ts"), "export {};\n");
    let widened = held().await;
    assert!(
        !Arc::ptr_eq(&regenerated, &widened),
        "a new directory stales it"
    );
}

/// The index walk runs to completion however long it takes; only its
/// memory bound (the index memo's byte budget) stops it, and an index cut
/// there says so: it is incomplete, and a request reading it reports the
/// ignored scan unfinished, with the files the walk reached.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_the_memory_bound_cuts_the_index_walk_and_a_cut_is_disclosed() {
    let (_dir, root) = fixture();
    write(&root.join(".gitignore"), "gen/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    for index in 0..40 {
        write(&root.join(format!("gen/g{index:02}.ts")), "f();\n");
    }
    let policy = workspace_policy(&root);
    let scope = scope_of(&root);
    let whole = build_within(&root, &scope.include, &policy, &|| false, INDEX_BYTES);
    assert!(whole.complete, "{whole:?}");
    assert_eq!(whole.files.len(), 40);
    assert!(whole.bytes > 0 && whole.bytes <= INDEX_BYTES);
    // A bound below what the whole index weighs cuts it.
    let cut = build_within(&root, &scope.include, &policy, &|| false, whole.bytes / 2);
    assert!(!cut.complete, "{cut:?}");
    assert!(
        cut.files.len() < 40 && cut.bytes <= whole.bytes / 2,
        "{cut:?}"
    );
    INDEXES.insert(scope.index_key(&policy), cut, 1, Some(INCOMPLETE_TTL));
    let found = scope
        .ignored_text_files(
            "f",
            &HashSet::new(),
            &policy,
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan");
    assert!(found.unfinished, "{found:?}");
}
