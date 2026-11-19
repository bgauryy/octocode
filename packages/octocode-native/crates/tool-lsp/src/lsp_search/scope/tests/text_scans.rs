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
    let cancel = octocode_runtime_base::execution::cancel::NeverCancel;
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
    let cancel = octocode_runtime_base::execution::cancel::NeverCancel;
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
async fn every_request_scans_its_current_source() {
    let (_dir, root) = fixture();
    write(&root.join("a.ts"), "export function f() {}\n");
    write(&root.join("b.ts"), "f();\n");
    let policy = workspace_policy(&root);
    let cancel = &octocode_runtime_base::execution::cancel::NeverCancel;
    let include = vec!["*.ts".to_owned()];
    let scan = || {
        let scope = Scope::new(root.to_string_lossy().into_owned(), include.clone());
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
    assert_eq!(scan().await, 2);
    // Every request scans its current source: no scan is shared.
    write(&root.join("c.ts"), "f();\n");
    assert_eq!(scan().await, 3);
    write(&root.join("d.ts"), "f();\n");
    assert_eq!(scan().await, 4);
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
        None,
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .await
    .expect("scan")
    .expect("files");
    // Every listed file spells the name here, so the two sets are one.
    assert_eq!((whole.found.len(), whole.listed.len()), (200, 200));
    assert!(whole.unsearched.is_none(), "no reach bounded the scan");
    let stopped = scan_text(root, &include, "f", &policy, None, &Stopped).await;
    assert!(stopped.is_err_and(|failure| failure.is_cancelled()));
}

struct Stopped;
impl octocode_runtime_base::execution::cancel::CancellationCheck for Stopped {
    fn check(&self) -> Result<(), String> {
        Err("deadline".into())
    }
}

/// The text scan reads what the ignore rules let a search see, as ripgrep and
/// editor search do: a gitignored file that spells the name is not among the
/// files it lists, while the visible one is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_scan_leaves_gitignored_files_out() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join(".gitignore"), "gen/\n");
    write(&root.join("a.ts"), "export function f() {}\n");
    write(&root.join("gen/g.ts"), "f();\n");
    let policy = workspace_policy(&root);
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()]);
    let files = scope
        .text_files(
            "f",
            &policy,
            &octocode_runtime_base::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan")
        .expect("files");
    let shown = |path: &str| canonical(&root.join(path).to_string_lossy());
    assert!(files.contains(&shown("a.ts")), "{files:?}");
    assert!(!files.contains(&shown("gen/g.ts")), "{files:?}");
}
