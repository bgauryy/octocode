use super::*;
use crate::tools::test_support::workspace_policy;
use std::collections::HashSet;

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

/// Every request selects its current ignored files and preserves exact name matches.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ignored_files_are_scanned_per_request_and_include_every_name() {
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

    // Another name independently selects the same source files.
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
    assert!(
        !Arc::ptr_eq(&index, &again),
        "each request owns its ignored-file scan"
    );

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

/// Saving a tracked file by rename changes its directory's stamp but adds
/// nothing ignored: the index holds. A new file that ignore rules hide in a
/// tracked directory, or a new directory, stales it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracked_edits_and_new_ignored_files_are_observed() {
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
    assert_eq!(
        first.files, saved.files,
        "tracked edits preserve the selected ignored files"
    );
    // A generated file the rules hide, beside tracked ones.
    write(&root.join("src/b.gen.ts"), "f();\n");
    let regenerated = held().await;
    assert!(
        !Arc::ptr_eq(&saved, &regenerated),
        "a hidden file is selected by a new scan"
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
        "each scan owns the current directory state"
    );
}

#[test]
fn a_cancelled_discovery_is_incomplete_and_a_new_request_reads_every_file() {
    let (_dir, root) = fixture();
    write(&root.join(".gitignore"), "gen/\n");
    for index in 0..120 {
        write(&root.join(format!("gen/g{index:03}.ts")), "f();\n");
    }
    let policy = workspace_policy(&root);
    let include = ["*.ts".to_owned()];
    let checks = std::sync::atomic::AtomicUsize::new(0);
    let cut = build(&root, &include, &policy, &|| {
        checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 10
    });
    assert!(!cut.complete);
    assert!(cut.files.len() < 120);
    let whole = build(&root, &include, &policy, &|| false);
    assert!(whole.complete);
    assert_eq!(
        whole.files,
        (0..120)
            .map(|index| root
                .join(format!("gen/g{index:03}.ts"))
                .to_string_lossy()
                .into_owned())
            .collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_get_complete_independent_scans_and_cancellation_is_an_error() {
    struct Ended;
    impl CancellationCheck for Ended {
        fn check(&self) -> Result<(), String> {
            Err("deadline".into())
        }
    }
    let (_dir, root) = fixture();
    write(&root.join(".gitignore"), "gen/\n");
    for index in 0..20 {
        write(&root.join(format!("gen/g{index}.ts")), "f();\n");
    }
    let policy = workspace_policy(&root);
    let scope = scope_of(&root);
    let skip = std::collections::HashSet::new();
    let ended = scope.ignored_text_files("f", &skip, &policy, &Ended).await;
    assert!(
        matches!(ended, Err(ref failure) if failure.code == octocode_contract_types::ErrorCode::Cancelled),
        "{ended:?}"
    );
    let answers = futures_util::future::join_all((0..6).map(|_| {
        scope.ignored_text_files("f", &skip, &policy, &crate::execution::cancel::NeverCancel)
    }))
    .await;
    for answer in answers {
        let answer = answer.expect("scan");
        assert!(!answer.unfinished);
        assert_eq!(answer.files.len(), 20);
    }
}
