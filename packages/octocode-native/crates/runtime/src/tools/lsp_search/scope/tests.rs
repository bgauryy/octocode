use super::*;
use crate::tools::test_support::workspace_policy;

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
    // A generous bound: the test measures what the digest covers, not
    // the walk's speed under a loaded test runner.
    let scope = |stopped: &(dyn Fn() -> bool + Sync)| {
        fingerprint(
            &root,
            &["*.ts".to_owned()],
            stopped,
            Duration::from_secs(60),
        )
    };
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
    let digest = || {
        fingerprint(
            &root,
            &["*.ts".to_owned()],
            &|| false,
            Duration::from_secs(60),
        )
        .expect("fingerprint")
    };
    let first = digest();
    write(&path, "export const a = 2;\n");
    set_old_time();
    assert_ne!(digest(), first);
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
