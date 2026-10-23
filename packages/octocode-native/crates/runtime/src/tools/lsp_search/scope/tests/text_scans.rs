//! Text scans: text-only files, parse bounds, reuse, ignored and lifted
//! scans.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn text_only_files_are_the_scanned_files_the_answer_misses() {
    let (_dir, root) = fixture();
    write(&root.join("a.ts"), "export function f() {}\n");
    write(&root.join("b.ts"), "import { f } from './a';\nf();\n");
    write(&root.join("c.ts"), "// f is mentioned here\n");
    write(&root.join("d.rs"), "fn f() {}\n");
    write(
        &root.join("e.ts"),
        "const label = 'f';\nexport const g = () => f();\n",
    );
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let policy = workspace_policy(&root);
    let cancel = crate::execution::cancel::NeverCancel;
    let files = scope
        .text_files("f", &policy, &cancel)
        .await
        .expect("scan")
        .expect("files");
    assert_eq!(files.len(), 4, "{files:?}");
    // Unknown until the operation records its answer.
    assert_eq!(scope.text_only_files("f"), None);
    scope.answer([root.join("a.ts").to_string_lossy()]);
    scope.answer([format!("file://{}", root.join("b.ts").display())]);
    // c.ts only mentions the name in a comment: no use was missed.
    assert_eq!(
        scope.text_only_files("f"),
        Some(vec![root.join("e.ts").to_string_lossy().into_owned()])
    );
    assert_eq!(scope.text_only_files("g"), None);
}

/// The text check parses at most [`CODE_CHECK_MAX_FILES`] unanswered
/// files, the bound the ignored-importer check shares; a file past it is
/// not proven free of a use, so it counts as one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn text_check_files_past_the_parse_bound_count_as_uses() {
    let (_dir, root) = fixture();
    write(&root.join("a.ts"), "export function f() {}\n");
    for index in 0..CODE_CHECK_MAX_FILES + 2 {
        write(
            &root.join(format!("c{index}.ts")),
            "// f is mentioned here\n",
        );
    }
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let files = scope
        .text_files(
            "f",
            &workspace_policy(&root),
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan")
        .expect("files");
    assert_eq!(files.len(), CODE_CHECK_MAX_FILES + 3, "{files:?}");
    scope.answer([root.join("a.ts").to_string_lossy()]);
    let uses = scope.text_only_files("f").expect("checked");
    assert_eq!(uses.len(), 2, "{uses:?}");
    assert!(uses.iter().all(|file| !file.ends_with("a.ts")), "{uses:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn text_scan_is_reused_under_an_unchanged_fingerprint() {
    let (_dir, root) = fixture();
    write(&root.join("a.ts"), "export function f() {}\n");
    write(&root.join("b.ts"), "f();\n");
    let policy = workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    let include = vec!["*.ts".to_owned()];
    let scan = |fingerprint: Option<&str>| {
        let scope = Scope::new(root.to_string_lossy().into_owned(), include.clone());
        if let Some(fingerprint) = fingerprint {
            scope.set_fingerprint(fingerprint.to_owned());
        }
        let policy = policy.clone();
        async move {
            scope
                .text_files("f", &policy, cancel)
                .await
                .expect("scan")
                .expect("files")
                .len()
        }
    };
    assert_eq!(scan(Some("fp")).await, 2);
    // A new mention under the same fingerprint string: the earlier
    // scan answers (the fingerprint is what proves the files unchanged),
    // so no file is read again.
    write(&root.join("c.ts"), "f();\n");
    assert_eq!(scan(Some("fp")).await, 2);
    // Another fingerprint, or none (walk past its bounds), scans again.
    assert_eq!(scan(Some("fp2")).await, 3);
    write(&root.join("d.ts"), "f();\n");
    assert_eq!(scan(None).await, 4);
}

/// A lifted scan that finishes lists the ignored files; one that stops at
/// its bound says so instead of reading as "no ignored file".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ignored_scan_past_its_bound_is_unfinished_not_empty() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join(".gitignore"), "gen/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    write(&root.join("b.ts"), "f();\n");
    for index in 0..50 {
        write(&root.join(format!("gen/g{index}.ts")), "f();\n");
    }
    let policy = workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    let skip = std::collections::HashSet::new();
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()]);

    let finished = scope
        .ignored_text_files("f", &skip, &policy, cancel)
        .await
        .expect("scan");
    assert!(!finished.unfinished);
    assert_eq!(finished.files.len(), 50);
    assert_eq!(finished.rules, [".gitignore:1 'gen/'"]);

    let stopped = scope
        .ignored_text_files_within("f", &skip, &policy, cancel, Duration::ZERO)
        .await
        .expect("scan");
    assert!(stopped.unfinished, "{stopped:?}");
    assert!(stopped.files.len() < 50, "{stopped:?}");
    // The visible files never count as ignored, finished or not.
    assert!(stopped.files.iter().all(|file| file.contains("/gen/")));
    // The scope's ignored tree outlasted the bound: later requests skip
    // the lifted scan, for any name, and report it unfinished.
    let skipped = scope
        .ignored_text_files("other", &skip, &policy, cancel)
        .await
        .expect("scan");
    assert!(
        skipped.unfinished && skipped.files.is_empty(),
        "{skipped:?}"
    );
}

/// The lifted scan does not enter a nested repository (a gitignored
/// checkout of another project): its files are not this repository's
/// ignored files, and the scan names the repository it left out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lifted_scan_skips_nested_repositories_and_names_them() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join(".gitignore"), "gen/\nrepos/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    write(&root.join("gen/g.ts"), "f();\n");
    write(
        &root.join("repos/other/.git/HEAD"),
        "ref: refs/heads/main\n",
    );
    write(&root.join("repos/other/x.ts"), "f();\n");
    let policy = workspace_policy(&root);
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()]);
    let found = scope
        .ignored_text_files(
            "f",
            &std::collections::HashSet::new(),
            &policy,
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan");
    assert!(!found.unfinished, "{found:?}");
    assert_eq!(
        found.files,
        [root.join("gen/g.ts").to_string_lossy().into_owned()],
        "{found:?}"
    );
    assert_eq!(found.nested_repositories, ["repos/other"], "{found:?}");
}

/// A warm request answers like the cold one: the symbol whose lifted
/// scan stopped at its bound replays the files that scan reached, still
/// unfinished, instead of reading as a skipped scan that found nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unfinished_ignored_scan_replays_what_it_reached() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join(".gitignore"), "gen/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    let reached = root.join("gen/g.ts").to_string_lossy().into_owned();
    write(Path::new(&reached), "f();\n");
    let policy = workspace_policy(&root);
    let cancel = &crate::execution::cancel::NeverCancel;
    let skip = std::collections::HashSet::new();
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()]);
    // What a cold scan reached before its bound (the bound is timing,
    // so the cold scan's reach is stated rather than raced).
    let (unbounded, replay) = scope.lifted_keys("f", &policy);
    UNBOUNDED_IGNORED.insert(unbounded, (), 1, Some(UNBOUNDED_IGNORED_TTL));
    UNFINISHED_LIFTED.insert(
        replay,
        Scanned {
            files: BTreeSet::from([reached.clone()]),
            complete: false,
            nested: Vec::new(),
        },
        1,
        Some(UNBOUNDED_IGNORED_TTL),
    );
    for _ in 0..2 {
        let warm = scope
            .ignored_text_files("f", &skip, &policy, cancel)
            .await
            .expect("scan");
        assert!(warm.unfinished, "{warm:?}");
        assert_eq!(warm.files, std::slice::from_ref(&reached), "{warm:?}");
        assert_eq!(warm.rules, [".gitignore:1 'gen/'"], "{warm:?}");
    }
}
