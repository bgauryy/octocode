//! Whole requests over a kept listing: the entries' answers, and the reads they
//! save, as `Scope` gives them.
use super::super::super::Scope;
use super::*;
use crate::tools::test_support::workspace_policy;
use std::collections::HashSet;

const NEVER: &crate::execution::cancel::NeverCancel = &crate::execution::cancel::NeverCancel;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, text).expect("write");
}

fn repository() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join(".gitignore"), "gen/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    write(&root.join("src/b.ts"), "export function g() {}\n");
    write(&root.join("gen/c.ts"), "f();\n");
    (dir, root)
}

fn scope(root: &Path) -> Scope {
    Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()])
}

async fn scan(root: &Path, name: &str) -> Vec<String> {
    let files = scope(root)
        .text_files(name, &workspace_policy(root), NEVER)
        .await
        .expect("scan")
        .expect("files");
    files
        .iter()
        .map(|file| {
            Path::new(file)
                .strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

async fn ignored(root: &Path, name: &str) -> Vec<String> {
    let found = scope(root)
        .ignored_text_files(name, &HashSet::new(), &workspace_policy(root), NEVER)
        .await
        .expect("scan");
    assert!(!found.unfinished, "{found:?}");
    found
        .files
        .iter()
        .map(|file| {
            Path::new(file)
                .strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// After one request has walked and one has read, each later request, for any
/// name, opens only the files that spell it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kept_scope_opens_only_the_files_that_spell_the_name() {
    let (_dir, root) = repository();
    let (a, b) = (root.join("a.ts"), root.join("src/b.ts"));
    settle_written_so_far();
    assert_eq!(scan(&root, "f").await, ["a.ts"], "the walk");
    assert_eq!(scan(&root, "f").await, ["a.ts"], "reads each file once");
    let (a_reads, b_reads) = (reads::of(&a), reads::of(&b));
    assert_eq!((a_reads, b_reads), (1, 1));

    assert!(scan(&root, "zzz").await.is_empty());
    assert_eq!((reads::of(&a), reads::of(&b)), (1, 1));
    assert_eq!(scan(&root, "g").await, ["src/b.ts"]);
    assert_eq!((reads::of(&a), reads::of(&b)), (1, 2));
    assert_eq!(scan(&root, "function").await, ["a.ts", "src/b.ts"]);
    assert_eq!((reads::of(&a), reads::of(&b)), (2, 3));
}

/// Whatever the list or a file's text does between two requests reaches the
/// second one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_change_reaches_the_next_request() {
    let (_dir, root) = repository();
    settle_written_so_far();
    assert_eq!(scan(&root, "f").await, ["a.ts"]);
    assert_eq!(scan(&root, "f").await, ["a.ts"]);

    write(&root.join("src/b.ts"), "export function g() {} f();\n");
    assert_eq!(scan(&root, "f").await, ["a.ts", "src/b.ts"], "a word added");
    write(&root.join("src/b.ts"), "export function g() {}\n");
    assert_eq!(scan(&root, "f").await, ["a.ts"], "a word removed");

    write(&root.join("src/new.ts"), "f();\n");
    assert_eq!(
        scan(&root, "f").await,
        ["a.ts", "src/new.ts"],
        "a file added"
    );
    std::fs::rename(root.join("src/new.ts"), root.join("src/renamed.ts")).expect("rename");
    assert_eq!(
        scan(&root, "f").await,
        ["a.ts", "src/renamed.ts"],
        "a file renamed"
    );
    std::fs::remove_file(root.join("src/renamed.ts")).expect("remove");
    assert_eq!(scan(&root, "f").await, ["a.ts"], "a file removed");
}

/// The ignored-file scan keeps its own rule: the same answers, read the same
/// way, with an unreadable file not proven free of the name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_ignored_scan_opens_only_the_files_that_spell_the_name() {
    let (_dir, root) = repository();
    let generated = root.join("gen/c.ts");
    settle_written_so_far();
    assert_eq!(ignored(&root, "f").await, ["gen/c.ts"]);
    assert_eq!(reads::of(&generated), 1);
    assert!(ignored(&root, "zzz").await.is_empty());
    assert_eq!(
        reads::of(&generated),
        1,
        "an absent name is answered unread"
    );
    assert_eq!(ignored(&root, "f").await, ["gen/c.ts"]);
    assert_eq!(reads::of(&generated), 2);

    write(&generated, "g();\n");
    assert!(ignored(&root, "f").await.is_empty());
    assert_eq!(ignored(&root, "g").await, ["gen/c.ts"]);
}
