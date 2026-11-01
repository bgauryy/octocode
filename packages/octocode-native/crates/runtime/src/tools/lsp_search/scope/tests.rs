use super::*;
use crate::tools::test_support::workspace_policy;
use std::time::Instant;

mod reach;
mod text_scans;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    (dir, root)
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, text).expect("write");
}

fn resolve(file: &Path, server_root: &Path, policy_root: &Path, id: &str) -> Scope {
    Scope::resolve(
        &file.to_string_lossy(),
        &server_root.to_string_lossy(),
        None,
        &workspace_policy(policy_root),
        Some(id),
    )
}

#[test]
fn scope_prefers_vcs_root_over_nearest_package() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(
        &root.join("package.json"),
        r#"{"workspaces":["packages/*"]}"#,
    );
    write(&root.join("packages/a/package.json"), "{}");
    let file = root.join("packages/a/src/x.ts");
    write(&file, "export const x = 1;\n");
    let scope = resolve(&file, &root.join("packages/a"), &root, "typescript");
    assert_eq!(scope.root, root.to_string_lossy());
    for glob in ["*.ts", "*.tsx", "*.js", "*.mts", "*.cjs"] {
        assert!(
            scope.include.iter().any(|g| g == glob),
            "{glob}: {:?}",
            scope.include
        );
    }
    assert!(!scope.include.iter().any(|g| g == "*.rs"));
}

#[test]
fn scope_without_vcs_uses_outermost_cargo_workspace() {
    let (_dir, root) = fixture();
    write(
        &root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"tokio\"]\n",
    );
    write(
        &root.join("tokio/Cargo.toml"),
        "[package]\nname = \"tokio\"\n",
    );
    let file = root.join("tokio/src/lib.rs");
    write(&file, "pub fn spawn_blocking() {}\n");
    let scope = resolve(&file, &root.join("tokio"), &root, "rust");
    assert_eq!(scope.root, root.to_string_lossy());
    assert_eq!(scope.include, vec!["*.rs".to_owned()]);
}

#[test]
fn scope_is_clamped_to_policy_root_and_never_cwd() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join("repo/.git")).expect("git");
    let file = root.join("repo/packages/a/x.ts");
    write(&file, "x\n");
    let packages = root.join("repo/packages");
    let scope = resolve(
        &file,
        &root.join("repo/packages/a"),
        &packages,
        "typescript",
    );
    assert_eq!(scope.root, packages.to_string_lossy());
    // No marker anywhere and a server root outside the policy: the
    // widest authorized directory on the file's path, not the cwd.
    let lone = root.join("lone/deep/y.py");
    write(&lone, "y = 1\n");
    let cwd = std::env::current_dir().expect("cwd");
    let scope = resolve(&lone, &cwd, &root.join("lone"), "python");
    assert_eq!(scope.root, root.join("lone").to_string_lossy());
    assert_eq!(scope.include, vec!["*.py".to_owned(), "*.pyi".to_owned()]);
}

#[test]
fn fingerprint_changes_on_edit_add_delete() {
    let (_dir, root) = fixture();
    write(&root.join("a.ts"), "export const a = 1;\n");
    write(&root.join("notes.md"), "notes\n");
    let include = ["*.ts".to_owned()];
    let walked = scan::FingerprintScope {
        root: &root,
        server_root: Some(&root),
        include: &include,
    };
    let scope = |stopped: &(dyn Fn() -> bool + Sync)| scan::fingerprint(&walked, stopped);
    let never = || false;
    let first = scope(&never).expect("fingerprint");
    assert_eq!(scope(&never).as_ref(), Some(&first));
    // An unrelated extension does not change it.
    write(&root.join("notes.md"), "other notes\n");
    assert_eq!(scope(&never).as_ref(), Some(&first));
    // A same-length edit within the mtime granularity still does.
    write(&root.join("a.ts"), "export const a = 2;\n");
    let edited = scope(&never).expect("edited");
    assert_ne!(edited, first);
    write(&root.join("b.ts"), "export const b = 1;\n");
    let added = scope(&never).expect("added");
    assert_ne!(added, edited);
    std::fs::remove_file(root.join("b.ts")).expect("remove");
    assert_eq!(scope(&never).as_ref(), Some(&edited));
    // A project file edit changes it too.
    write(&root.join("tsconfig.json"), "{}");
    assert_ne!(scope(&never).as_ref(), Some(&edited));
    // A stopped walk reports no fingerprint.
    assert_eq!(scope(&|| true), None);
}

#[test]
fn fingerprint_changes_after_same_size_edit_with_restored_old_mtime() {
    let (_dir, root) = fixture();
    let path = root.join("a.ts");
    write(&path, "export const a = 1;\n");
    let old_time = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let set_old_time = || {
        std::fs::File::open(&path)
            .expect("file")
            .set_times(std::fs::FileTimes::new().set_modified(old_time))
            .expect("mtime");
    };
    set_old_time();
    let include = ["*.ts".to_owned()];
    let walked = scan::FingerprintScope {
        root: &root,
        server_root: Some(&root),
        include: &include,
    };
    let digest = || scan::fingerprint(&walked, &|| false).expect("fingerprint");
    let first = digest();
    write(&path, "export const a = 2;\n");
    set_old_time();
    assert_ne!(digest(), first);
}

/// Queries that ask for one scope's fingerprint while its walk runs (a
/// batch's rows start together) share that one walk and its digest; a
/// request asking after it ends walks again and sees an edit made since.
#[test]
fn concurrent_requests_share_one_fingerprint_walk_of_a_scope() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    const REQUESTS: usize = 5;
    let (_dir, root) = fixture();
    for index in 0..40 {
        write(&root.join(format!("m{index}.ts")), "export const x = 1;\n");
    }
    let include = ["*.ts".to_owned()];
    let walked_scope = scan::FingerprintScope {
        root: &root,
        server_root: Some(&root),
        include: &include,
    };
    let walked = (0..REQUESTS)
        .map(|_| AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let started = std::sync::Barrier::new(REQUESTS);
    let digests = std::thread::scope(|threads| {
        let requests = walked.iter().map(|calls| {
            let (scope, started) = (&walked_scope, &started);
            threads.spawn(move || {
                started.wait();
                // The walking request holds its walk until every request
                // has joined it (the table, the walker and each joiner
                // hold the flight), so none can arrive after it ends.
                let stopped = || {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        let deadline = Instant::now() + Duration::from_secs(20);
                        while scan::flight_holders(scope) < REQUESTS + 1
                            && Instant::now() < deadline
                        {
                            std::thread::yield_now();
                        }
                    }
                    false
                };
                scan::shared_fingerprint(scope, &stopped).expect("fingerprint")
            })
        });
        requests
            .collect::<Vec<_>>()
            .into_iter()
            .map(|request| request.join().expect("request"))
            .collect::<Vec<_>>()
    });
    assert!(digests.iter().all(|digest| digest.0 == digests[0].0));
    let walkers = walked
        .iter()
        .filter(|calls| calls.load(Ordering::SeqCst) > 0)
        .count();
    assert_eq!(walkers, 1, "one walk for the requests that asked at once");
    assert_eq!(
        scan::flight_holders(&walked_scope),
        0,
        "nothing in flight after"
    );
    write(&root.join("m0.ts"), "export const x = 22;\n");
    let after = scan::shared_fingerprint(&walked_scope, &|| false).expect("again");
    assert_ne!(after.0, digests[0].0, "a later request walks again");
}

/// The widest authorized directory on a path: the policy root for a
/// file under it, nothing for a path outside every root.
#[test]
fn the_widest_authorized_dir_is_the_policy_root_on_the_path() {
    let (_dir, root) = fixture();
    write(&root.join("ws/crates/a/src/lib.rs"), "fn f() {}\n");
    let policy = workspace_policy(&root.join("ws/crates"));
    let canonical_crates = root.join("ws/crates").to_string_lossy().into_owned();
    assert_eq!(
        widest_authorized_dir(&root.join("ws/crates/a/src"), &policy),
        Some(canonical_crates)
    );
    assert_eq!(widest_authorized_dir(&root.join("ws"), &policy), None);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_paths_are_memoized_per_request() {
    let (_dir, root) = fixture();
    write(&root.join("one/x.ts"), "x\n");
    write(&root.join("two/x.ts"), "x\n");
    let link = root.join("link");
    std::os::unix::fs::symlink(root.join("one"), &link).expect("link");
    let spelled = link.join("x.ts").to_string_lossy().into_owned();
    let first = root.join("one/x.ts").to_string_lossy().into_owned();
    let second = root.join("two/x.ts").to_string_lossy().into_owned();
    let seen = with_canonical_memo(async {
        let before = canonical(&spelled);
        std::fs::remove_file(&link).expect("unlink");
        std::os::unix::fs::symlink(root.join("two"), &link).expect("relink");
        (before, canonical(&spelled))
    })
    .await;
    // One request resolves a spelling once; the next request sees the
    // repointed link.
    assert_eq!(seen, (first.clone(), first));
    assert_eq!(
        with_canonical_memo(async { canonical(&spelled) }).await,
        second
    );
    assert_eq!(canonical(&spelled), second);
}

#[test]
fn an_explicit_workspace_root_holding_the_anchor_bounds_the_scope() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    let package = root.join("packages/a");
    let file = package.join("src/x.ts");
    write(&file, "export const x = 1;\n");
    let policy = workspace_policy(&root);
    let scoped = |workspace_root: Option<&Path>| {
        let workspace_root = workspace_root.map(|root| root.to_string_lossy().into_owned());
        Scope::resolve(
            &file.to_string_lossy(),
            &package.to_string_lossy(),
            workspace_root.as_deref(),
            &policy,
            Some("typescript"),
        )
    };
    let inner = scoped(Some(&package));
    assert_eq!(inner.root, package.to_string_lossy());
    assert!(inner.narrowed, "narrower than the repository root");
    let whole = scoped(Some(&root));
    assert_eq!(whole.root, root.to_string_lossy());
    assert!(!whole.narrowed);
    let default = scoped(None);
    assert_eq!(default.root, root.to_string_lossy());
    assert!(!default.narrowed);
    // A root that does not hold the anchor leaves the default scope.
    let elsewhere = root.join("packages/b");
    std::fs::create_dir_all(&elsewhere).expect("b");
    let other = scoped(Some(&elsewhere));
    assert_eq!(other.root, root.to_string_lossy());
    assert!(!other.narrowed);
}

/// A request's fingerprint walk is traced (`lsp.trace`): one that found
/// the fingerprint completes, and one that ended without it (here the
/// request was cancelled) is counted as failed, never dropped silently.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fingerprint_walk_without_a_fingerprint_is_counted_in_the_trace() {
    struct Cancelled(bool);
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            if self.0 {
                Err("cancelled".to_owned())
            } else {
                Ok(())
            }
        }
    }
    let (_dir, root) = fixture();
    write(&root.join("a.ts"), "export const a = 1;\n");
    let scope = resolve(&root.join("a.ts"), &root, &root, "typescript");
    let trace = octocode_engine::lsp::diagnostics::Trace::default();
    let found = trace.run(scope.walk_fingerprint(&Cancelled(false))).await;
    assert!(found.expect("walked").is_some());
    assert!(
        trace.summary().contains("scopeFingerprint: ") && trace.summary().contains("completed=1"),
        "{}",
        trace.summary()
    );
    let trace = octocode_engine::lsp::diagnostics::Trace::default();
    let stopped = trace.run(scope.walk_fingerprint(&Cancelled(true))).await;
    assert!(!matches!(stopped, Ok(Some(_))));
    let summary = trace.summary();
    assert!(
        summary.contains("scopeFingerprint: ") && summary.contains("failed=1"),
        "{summary}"
    );
}

/// The fingerprint covers what can change an answer: the scope's files its
/// ignore rules admit (what the text scans read), and every file under the
/// server's root, ignored ones included (the server may compile a
/// generated source). An ignored file outside the server's root (a cloned
/// fixture tree, a scratch copy) changes neither, so it is not walked.
#[test]
fn the_fingerprint_walks_ignored_files_only_under_the_server_root() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join(".gitignore"), "gen/\nclones/\n");
    write(&root.join("app/src/lib.rs"), "pub fn a() {}\n");
    write(&root.join("app/gen/out.rs"), "pub fn g() {}\n");
    write(&root.join("clones/other/x.rs"), "pub fn x() {}\n");
    let include = ["*.rs".to_owned()];
    let server = root.join("app");
    let scope = scan::FingerprintScope {
        root: &root,
        server_root: Some(&server),
        include: &include,
    };
    let (first, entries) = scan::fingerprint(&scope, &|| false).expect("fingerprint");
    let walked = entries.keys().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(walked, ["app/gen/out.rs", "app/src/lib.rs"], "{walked:?}");
    write(
        &root.join("clones/other/x.rs"),
        "pub fn x() { changed() }\n",
    );
    assert_eq!(
        scan::fingerprint(&scope, &|| false).expect("again").0,
        first
    );
    write(&root.join("app/gen/out.rs"), "pub fn g() { changed() }\n");
    assert_ne!(
        scan::fingerprint(&scope, &|| false).expect("again").0,
        first
    );
}
