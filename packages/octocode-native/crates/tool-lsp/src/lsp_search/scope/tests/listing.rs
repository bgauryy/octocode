//! A scope's file listing outlives its request while a real check proves the
//! walk would list the same files: any name is then searched without a walk,
//! over the files as they are now.
use super::super::probe::{Work, measure};
use super::*;
use octocode_engine::stamp::settle_written_so_far;
use std::sync::Arc;

const NEVER: &octocode_runtime_base::execution::cancel::NeverCancel =
    &octocode_runtime_base::execution::cancel::NeverCancel;

/// The relative files a fresh request's text scan of `name` finds, and the
/// walks it made.
async fn scan(root: &Path, name: &str) -> (Vec<String>, usize) {
    scan_under(root, name, &["*.ts"]).await
}

/// [`scan`] of a scope whose walk reads the files `include` names.
async fn scan_under(root: &Path, name: &str, include: &[&str]) -> (Vec<String>, usize) {
    let scope = Scope::new(
        root.to_string_lossy().into_owned(),
        include.iter().map(|glob| (*glob).to_owned()).collect(),
    );
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

/// The text scan prunes the directories every local walk prunes
/// (`policy::prune`; outside a git work tree, a listed name wherever it sits):
/// a build copy in a non-ignored `dist/` is not listed, so it never counts as
/// a text-only file. An include that names `dist/**` lifts that prune, as in
/// the other tools. The kept listing proves the pruned walk like any other.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_scan_prunes_the_directories_every_local_walk_prunes() {
    let (_dir, root) = fixture();
    write(&root.join("src/lib.rs"), "pub fn f() {}\n");
    write(&root.join("dist/copy.rs"), "pub fn f() {}\n");
    settle_written_so_far();
    assert_eq!(
        scan_under(&root, "f", &["*.rs"]).await,
        (vec!["src/lib.rs".into()], 1)
    );
    assert_eq!(
        scan_under(&root, "f", &["*.rs"]).await,
        (vec!["src/lib.rs".into()], 0),
        "the pruned walk's listing is kept"
    );
    assert_eq!(
        scan_under(&root, "f", &["*.rs", "dist/**"]).await,
        (vec!["dist/copy.rs".into(), "src/lib.rs".into()], 1)
    );
}

/// One request stats each path once (TE9): its kept fingerprint's proof
/// stamps every file and directory, and the text scan over the kept listing
/// (the listing's proof, then a spelling lookup per file) stats none of them
/// again: the fingerprint's held proof answers both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_stats_each_path_once() {
    use crate::lsp_search::scope::spelling_index::STATS;
    let (_dir, root) = repository();
    for index in 0..24 {
        write(&root.join(format!("src/f{index:02}.ts")), "f();\n");
    }
    settle_written_so_far();
    let shown = root.to_string_lossy().into_owned();
    let policy = workspace_policy(&root);
    // Keep the scope's fingerprint, its listing and the files' entries.
    let warm = Scope::new(shown.clone(), vec!["*.ts".into()]);
    let clock = octocode_engine::stamp::settle_clock();
    assert!(
        warm.walk_fingerprint(clock, NEVER)
            .await
            .expect("walk")
            .is_some()
    );
    warm.text_files("f", &policy, NEVER).await.expect("scan");
    warm.text_files("g", &policy, NEVER).await.expect("entries");
    let listed = warm.listed_files("g").expect("listed");
    assert!(listed.len() > 24, "{listed:?}");
    let stats = |files: &[PathBuf]| files.iter().map(|file| STATS.of(file)).sum::<usize>();
    let before = stats(&listed);

    let request = Scope::new(shown.clone(), vec!["*.ts".into()]);
    let work = Arc::new(Work::default());
    let stamped = measure(
        &work,
        crate::lsp_search::observation::with_observation(async {
            assert!(
                request
                    .walk_fingerprint(clock, NEVER)
                    .await
                    .expect("kept")
                    .is_some()
            );
            let stamped = work.observed.stats();
            let found = request.text_files("g", &policy, NEVER).await.expect("scan");
            assert_eq!(found.map(|files| files.len()), Some(1), "a.ts");
            stamped
        }),
    )
    .await;
    assert_eq!(Work::get(&work.walks), 1, "the request's kept fingerprint");
    assert_eq!(Work::get(&work.text_scans), 0, "the kept listing");
    assert!(stamped > listed.len(), "the fingerprint stamped every file");
    assert_eq!(stats(&listed), before, "no spelling lookup stat'ed a file");
    assert!(
        work.observed.stats() - stamped < listed.len() / 2,
        "the listing's proof reads only what the fingerprint does not stamp: {} for {} files",
        work.observed.stats() - stamped,
        listed.len()
    );

    // Without the fingerprint's held proof, the same scan stats each file.
    let alone = Scope::new(shown, vec!["*.ts".into()]);
    let work = Arc::new(Work::default());
    measure(
        &work,
        crate::lsp_search::observation::with_observation(async {
            alone.text_files("g", &policy, NEVER).await.expect("scan");
        }),
    )
    .await;
    assert_eq!(stats(&listed), before + listed.len());
}
