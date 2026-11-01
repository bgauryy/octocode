//! Text scans: text-only files, parse bounds, reuse, and ignored files
//! (the ignored-file index).
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

/// Every unanswered file is checked for a code use, however many: one that
/// only mentions the name in a comment is never counted as text-only.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_unanswered_file_is_checked_for_a_code_use() {
    let (_dir, root) = fixture();
    write(&root.join("a.ts"), "export function f() {}\n");
    for index in 0..100 {
        write(
            &root.join(format!("c{index}.ts")),
            "// f is mentioned here\n",
        );
    }
    write(&root.join("z.ts"), "f();\n");
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let cancel = crate::execution::cancel::NeverCancel;
    let files = scope
        .text_files("f", &workspace_policy(&root), &cancel)
        .await
        .expect("scan")
        .expect("files");
    assert_eq!(files.len(), 102, "{files:?}");
    scope.answer([root.join("a.ts").to_string_lossy()]);
    scope.check_code_uses("f", &cancel).await.expect("checked");
    assert_eq!(
        scope.text_only_files("f"),
        Some(vec![root.join("z.ts").to_string_lossy().into_owned()])
    );
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

/// The ignored-file index does not enter a nested repository (a gitignored
/// checkout of another project): its files are not this repository's
/// ignored files, and the scan names the repository it left out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_ignored_index_skips_nested_repositories_and_names_them() {
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

/// The scan has no time bound of its own: it walks the whole scope, and
/// only the request stops it, keeping nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_walks_the_whole_scope_until_its_request_stops_it() {
    let (_dir, root) = fixture();
    for index in 0..200 {
        write(&root.join(format!("d{}/a{index}.ts", index % 7)), "f();\n");
    }
    let policy = workspace_policy(&root);
    let include = ["*.ts".to_owned()];
    let root = root.to_str().expect("utf-8 root");
    let whole = scan_text(
        root,
        &include,
        "f",
        &policy,
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("scan")
    .expect("files");
    assert_eq!(whole.len(), 200);
    let stopped = scan_text(root, &include, "f", &policy, &Stopped).await;
    assert!(stopped.is_err_and(|failure| failure.is_cancelled()));
}

struct Stopped;
impl crate::execution::cancel::CancellationCheck for Stopped {
    fn check(&self) -> Result<(), String> {
        Err("deadline".into())
    }
}
