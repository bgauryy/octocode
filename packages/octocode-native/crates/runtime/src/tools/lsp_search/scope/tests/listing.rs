//! A scope's file listing outlives its request while a real check proves the
//! walk would list the same files: any name is then searched without a walk,
//! over the files as they are now.
use super::super::probe::{Work, measure};
use super::*;
use octocode_engine::stamp::settle_written_so_far;
use std::collections::HashSet;
use std::sync::Arc;

const NEVER: &crate::execution::cancel::NeverCancel = &crate::execution::cancel::NeverCancel;

/// The relative files a fresh request's text scan of `name` finds, and the
/// walks it made.
async fn scan(root: &Path, name: &str) -> (Vec<String>, usize) {
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let work = Arc::new(Work::default());
    let files = measure(
        &work,
        scope.text_files(name, &workspace_policy(root), NEVER),
    )
    .await
    .expect("scan")
    .expect("files");
    let relative = files
        .iter()
        .map(|file| {
            Path::new(file)
                .strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    (relative, Work::get(&work.text_scans))
}

fn repository() -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join(".gitignore"), "gen/\n");
    write(
        &root.join("a.ts"),
        "export function f() {}\nexport function g() {}\n",
    );
    write(&root.join("src/b.ts"), "f();\n");
    write(&root.join("gen/c.ts"), "f();\n");
    (dir, root)
}

/// Once the scope is settled, the first request's walk serves every later
/// request and every other name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settled_listing_serves_every_name_without_a_walk() {
    let (_dir, root) = repository();
    settle_written_so_far();
    assert_eq!(
        scan(&root, "f").await,
        (vec!["a.ts".into(), "src/b.ts".into()], 1)
    );
    assert_eq!(
        scan(&root, "f").await,
        (vec!["a.ts".into(), "src/b.ts".into()], 0)
    );
    assert_eq!(scan(&root, "g").await, (vec!["a.ts".into()], 0));
    assert_eq!(scan(&root, "zzz").await, (Vec::new(), 0));
}

/// A scope written within the settle window could change unseen in the same
/// clock tick, so its walk is not kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unsettled_scope_walks_each_request() {
    let (_dir, root) = repository();
    assert_eq!(scan(&root, "g").await.1, 1);
    assert_eq!(scan(&root, "g").await.1, 1);
}

/// Every change between two requests reaches the second: a listed file's
/// text without a walk, anything that changes the list with one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reused_listing_answers_for_the_scope_as_it_is_now() {
    let (_dir, root) = repository();
    settle_written_so_far();
    assert_eq!(scan(&root, "f").await.1, 1);

    // An edit of a listed file.
    write(&root.join("src/b.ts"), "g();\n");
    assert_eq!(scan(&root, "f").await, (vec!["a.ts".into()], 0));
    assert_eq!(
        scan(&root, "g").await,
        (vec!["a.ts".into(), "src/b.ts".into()], 0)
    );

    // A new file in a listed directory.
    write(&root.join("src/d.ts"), "f();\n");
    assert_eq!(
        scan(&root, "f").await,
        (vec!["a.ts".into(), "src/d.ts".into()], 1)
    );

    // A removed file.
    std::fs::remove_file(root.join("src/d.ts")).expect("remove");
    assert_eq!(scan(&root, "f").await, (vec!["a.ts".into()], 1));

    // A new directory.
    write(&root.join("lib/e.ts"), "f();\n");
    assert_eq!(
        scan(&root, "f").await,
        (vec!["a.ts".into(), "lib/e.ts".into()], 1)
    );

    // An edited ignore file admits what it hid.
    write(&root.join(".gitignore"), "lib/\n");
    assert_eq!(
        scan(&root, "f").await,
        (vec!["a.ts".into(), "gen/c.ts".into()], 1)
    );
}

/// The stamps that prove the list cover what decides it beyond the walked
/// directories: a nested ignore file (edited in place, so its directory keeps
/// its stamp), the repository's own exclude file, and the marker that makes
/// `.gitignore` apply at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_falls_with_any_ignore_input() {
    let (_dir, root) = repository();
    write(&root.join("src/.gitignore"), "skipped.ts\n");
    write(&root.join("src/skipped.ts"), "f();\n");
    write(&root.join(".git/info/exclude"), "\n");
    settle_written_so_far();
    let walks = |found: (Vec<String>, usize)| found.1;
    assert_eq!(walks(scan(&root, "f").await), 1);
    assert_eq!(walks(scan(&root, "f").await), 0);

    write(&root.join("src/.gitignore"), "other.ts\n");
    assert_eq!(walks(scan(&root, "f").await), 1);

    settle_written_so_far();
    assert_eq!(
        walks(scan(&root, "f").await),
        1,
        "the stored walk was unsettled"
    );
    assert_eq!(walks(scan(&root, "f").await), 0);
    write(&root.join(".git/info/exclude"), "src/\n");
    let (files, made) = scan(&root, "f").await;
    assert_eq!(made, 1);
    assert!(!files.contains(&"src/b.ts".to_owned()), "{files:?}");

    settle_written_so_far();
    assert_eq!(walks(scan(&root, "f").await), 1);
    assert_eq!(walks(scan(&root, "f").await), 0);
    std::fs::remove_dir_all(root.join(".git")).expect("no repository");
    let (files, made) = scan(&root, "f").await;
    assert_eq!(made, 1, "without a repository .gitignore does not apply");
    assert!(files.contains(&"gen/c.ts".to_owned()), "{files:?}");
}

/// The search over a listing finds what the walk's search finds, on every
/// kind of text a scope holds: boundaries, encodings and binary files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_search_and_a_walk_agree_on_every_file() {
    let (_dir, root) = repository();
    let utf16 = "f();\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let cases: [(&str, &[u8]); 16] = [
        ("cases/plain.ts", b"f();\n"),
        ("cases/inside.ts", b"xfx();\n"),
        ("cases/dollar.ts", b"$f();\n"),
        ("cases/unicode_before.ts", "éf();\n".as_bytes()),
        ("cases/unicode_after.ts", "fé();\n".as_bytes()),
        ("cases/comment.ts", b"// f\n"),
        ("cases/string.ts", b"const s = 'f';\n"),
        ("cases/no_line_end.ts", b"f"),
        ("cases/crlf.ts", b"a\r\nf\r\nb\r\n"),
        ("cases/empty.ts", b""),
        ("cases/nul_after.ts", b"f();\n\0\n"),
        ("cases/nul_before.ts", b"line\n\0\nf();\n"),
        ("cases/binary.ts", b"\0\x01\x02 f \x03"),
        ("cases/latin1.ts", b"\xe9 f();\n"),
        ("cases/notes.md", b"f\n"),
        (".hidden/h.ts", b"f();\n"),
    ];
    for (file, text) in cases {
        std::fs::create_dir_all(root.join(file).parent().expect("parent")).expect("dirs");
        std::fs::write(root.join(file), text).expect("write");
    }
    let mut bom = vec![0xff, 0xfe];
    bom.extend(utf16);
    std::fs::write(root.join("cases/utf16.ts"), bom).expect("write");
    write(&root.join(".gitignore"), "gen/\n*.gen.ts\n");
    write(&root.join("cases/picked.gen.ts"), "f();\n");
    let (walked, made) = scan(&root, "f").await;
    assert_eq!(made, 1);
    settle_written_so_far();
    assert_eq!(scan(&root, "f").await.1, 1, "this walk stores the listing");
    let (listed, made) = scan(&root, "f").await;
    assert_eq!(made, 0, "searched from the listing");
    assert_eq!(listed, walked);
    let (indexed, made) = scan(&root, "f").await;
    assert_eq!(made, 0);
    assert_eq!(indexed, walked, "answered from what each file spells");
    let (other, _) = scan(&root, "xfx").await;
    assert_eq!(other, ["cases/inside.ts"]);
}

/// A policy decides what a walk may list: another one's listing is not its.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_is_kept_per_path_policy() {
    let (_dir, root) = repository();
    let inner = workspace_policy(&root.join("src"));
    settle_written_so_far();
    assert_eq!(scan(&root, "f").await.1, 1);
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let work = Arc::new(Work::default());
    measure(&work, scope.text_files("f", &inner, NEVER))
        .await
        .expect("scan");
    assert_eq!(Work::get(&work.text_scans), 1);
}

/// The ignored-file index outlives its request the same way: once settled it
/// is not walked again, and any name reads the ignored files as they are now.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settled_ignored_index_serves_later_requests_and_falls_with_any_change() {
    let (_dir, root) = repository();
    write(&root.join("repos/other/.git/HEAD"), "ref\n");
    write(&root.join("repos/other/x.ts"), "f();\n");
    write(&root.join(".gitignore"), "gen/\nrepos/\n");
    settle_written_so_far();
    let policy = workspace_policy(&root);
    let scope = || Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let ignored = |name: &'static str| {
        let (scope, policy) = (scope(), policy.clone());
        async move {
            let work = Arc::new(Work::default());
            let skip = HashSet::new();
            let found = measure(&work, scope.ignored_text_files(name, &skip, &policy, NEVER))
                .await
                .expect("scan");
            assert!(!found.unfinished, "{found:?}");
            (found.files.len(), Work::get(&work.ignored_walks))
        }
    };
    assert_eq!(ignored("f").await, (1, 1));
    assert_eq!(ignored("f").await, (1, 0));
    assert_eq!(ignored("g").await, (0, 0));

    // An ignored file's text is read as it is now.
    write(&root.join("gen/c.ts"), "g();\n");
    assert_eq!(ignored("f").await, (0, 0));
    assert_eq!(ignored("g").await, (1, 0));

    // A new ignored file changes its directory.
    write(&root.join("gen/d.ts"), "f();\n");
    assert_eq!(ignored("f").await, (1, 1));

    // A nested repository that stops being one is walked.
    settle_written_so_far();
    assert_eq!(ignored("f").await, (1, 1));
    assert_eq!(ignored("f").await, (1, 0));
    std::fs::remove_dir_all(root.join("repos/other/.git")).expect("no nested repository");
    assert_eq!(ignored("f").await, (2, 1));
}
