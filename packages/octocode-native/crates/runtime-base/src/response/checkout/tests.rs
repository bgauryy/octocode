use super::*;
use serde_json::json;
use std::fs;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER: &str = "89abcdef0123456789abcdef0123456789abcdef";

/// A work tree at `dir` whose `HEAD` holds `head`, with one source file.
fn work_tree(dir: &Path, head: &str) -> String {
    fs::create_dir_all(dir.join(".git")).expect("git dir");
    fs::write(dir.join(".git/HEAD"), format!("{head}\n")).expect("HEAD");
    fs::create_dir_all(dir.join("src")).expect("src");
    let file = dir.join("src/lib.rs");
    fs::write(&file, "fn main() {}\n").expect("source");
    file.to_string_lossy().into_owned()
}

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("workspace");
    let root = dir.path().canonicalize().expect("canonical workspace");
    (dir, root)
}

#[test]
fn a_row_in_one_work_tree_states_its_checked_out_commit() {
    let (_dir, root) = workspace();
    let file = work_tree(&root.join("repo"), SHA);
    let paths = crate::test_support::workspace_policy(&root);
    let row = json!({"files":[{"path":file,"matches":[{"line":1}]}]});
    assert_eq!(
        checkout_commit(&row, &paths, &Value::Null).as_deref(),
        Some(SHA)
    );
    // A row naming no path takes the asked one; an error row states none.
    let asked = json!({"path": root.join("repo/src").to_string_lossy()});
    assert_eq!(
        checkout_commit(&json!({"matchCount":0}), &paths, &asked).as_deref(),
        Some(SHA)
    );
}

/// A branch checkout with an `origin` remote: `HEAD` on `branch`, the ref
/// loose, the remote's `url` as `url` writes it in the config.
fn clone_of(dir: &Path, branch: &str, url: &str) -> String {
    let file = work_tree(dir, &format!("ref: refs/heads/{branch}"));
    let reference = dir.join(".git/refs/heads").join(branch);
    fs::create_dir_all(reference.parent().expect("ref folder")).expect("refs");
    fs::write(reference, format!("{SHA}\n")).expect("ref");
    fs::write(
        dir.join(".git/config"),
        format!(
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://example.com/up/stream.git\n[remote \"origin\"]\n\turl = {url}\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n[branch \"{branch}\"]\n\tremote = origin\n"
        ),
    )
    .expect("config");
    file
}

fn stated(row: &Value, paths: &PathPolicy, asked: &Value) -> Value {
    let mut fields = Map::new();
    if let Some(checkout) = checkout(row, paths, asked) {
        checkout.state(&mut fields);
    }
    Value::Object(fields)
}

#[test]
fn a_branch_checkout_states_its_commit_branch_and_origin() {
    let (_dir, root) = workspace();
    let file = clone_of(
        &root.join("repo"),
        "feature/x",
        "https://github.com/django/django.git",
    );
    let detached = work_tree(&root.join("detached"), SHA);
    let paths = crate::test_support::workspace_policy(&root);
    assert_eq!(
        stated(&json!({"path":file}), &paths, &Value::Null),
        json!({"commitSha":SHA,"ref":"feature/x","remote":"github.com/django/django"})
    );
    // A detached HEAD is on no branch, and no remote is no repository:
    // neither is guessed.
    assert_eq!(
        stated(&json!({"path":detached}), &paths, &Value::Null),
        json!({"commitSha":SHA})
    );
}

/// An error row names no evidence, so the path it was asked decides: a
/// refused read inside `.git` states the checkout it was after.
#[test]
fn an_error_row_states_the_checkout_of_the_asked_path() {
    let (_dir, root) = workspace();
    let file = clone_of(
        &root.join("repo"),
        "main",
        "git@github.com:django/django.git",
    );
    let paths = crate::test_support::workspace_policy(&root);
    let on_file = stated(&json!({"path":file}), &paths, &Value::Null);
    for asked in [
        "repo/.git",
        "repo/.git/config",
        "repo/.git/refs/heads",
        "repo/src/gone.rs",
    ] {
        let asked = json!({"path": root.join(asked).to_string_lossy()});
        // The row's own `path` is a shown name, not evidence.
        let refused = json!({"errorCode":"pathPolicyDenied","path":"elsewhere/.git"});
        assert_eq!(stated(&refused, &paths, &asked), on_file, "{asked}");
    }
    assert!(
        on_file["remote"].is_string() && on_file["ref"].is_string(),
        "{on_file}"
    );
}

/// Whatever form the remote takes, only its host and path are stated: user
/// information, a scheme, a query or a fragment never leave the config.
#[test]
fn a_remote_is_stated_as_host_and_path_without_user_information() {
    const SECRET: &str = "ghp_tok3nValue";
    let (_dir, root) = workspace();
    let urls = [
        format!("https://user:{SECRET}@GitHub.com/o/r.git"),
        format!("https://{SECRET}@github.com/o/r"),
        format!("https://user:p@ss{SECRET}@github.com/o/r.git/"),
        format!("ssh://git:{SECRET}@github.com/o/r.git"),
        format!("{SECRET}@github.com:o/r.git"),
        format!("https://github.com/o/r.git?token={SECRET}"),
        format!("https://github.com/o/r#{SECRET}"),
        format!("\"https://user:{SECRET}@github.com/o/r.git\" ; {SECRET}"),
        "git://github.com/o/r.git # the mirror".to_owned(),
    ];
    let paths = crate::test_support::workspace_policy(&root);
    for (index, url) in urls.iter().enumerate() {
        let file = clone_of(&root.join(format!("repo{index}")), "main", url);
        let found = checkout(&json!({"path":file}), &paths, &Value::Null).expect("checkout");
        assert_eq!(found.remote.as_deref(), Some("github.com/o/r"), "{url}");
    }
    // A remote on this machine names a path, never a repository; a user
    // name with nothing after it names none either.
    for (index, url) in [
        "/srv/git/r.git",
        "../r",
        "file:///srv/git/r.git",
        "C:\\git\\r",
        "user@host",
    ]
    .iter()
    .enumerate()
    {
        let file = clone_of(&root.join(format!("local{index}")), "main", url);
        let found = checkout(&json!({"path":file}), &paths, &Value::Null).expect("checkout");
        assert_eq!(found.remote, None, "{url}");
        assert_eq!(found.reference.as_deref(), Some("main"), "{url}");
    }
}

/// A row that states its own commit keeps it; a branch and remote read with
/// another commit would describe a different state, so they stay unsaid.
#[test]
fn a_row_with_its_own_commit_takes_no_branch_or_remote_of_another() {
    let found = Checkout {
        commit_sha: SHA.to_owned(),
        reference: Some("main".to_owned()),
        remote: Some("github.com/o/r".to_owned()),
    };
    let mut other = json!({"commitSha": OTHER});
    found.clone().state(other.as_object_mut().expect("row"));
    assert_eq!(other, json!({"commitSha": OTHER}));
    let mut same = json!({"commitSha": SHA});
    found.state(same.as_object_mut().expect("row"));
    assert_eq!(
        same,
        json!({"commitSha":SHA,"ref":"main","remote":"github.com/o/r"})
    );
}

#[test]
fn a_branch_resolves_from_its_loose_ref_or_packed_refs() {
    let (_dir, root) = workspace();
    let loose = work_tree(&root.join("loose"), "ref: refs/heads/main");
    fs::create_dir_all(root.join("loose/.git/refs/heads")).expect("refs");
    fs::write(root.join("loose/.git/refs/heads/main"), format!("{SHA}\n")).expect("ref");
    let packed = work_tree(&root.join("packed"), "ref: refs/heads/dev");
    fs::write(
        root.join("packed/.git/packed-refs"),
        format!(
            "# pack-refs with: peeled\n{SHA} refs/heads/main\n{OTHER} refs/heads/dev\n^{SHA}\n"
        ),
    )
    .expect("packed-refs");
    let paths = crate::test_support::workspace_policy(&root);
    let commit = |file: &str| checkout_commit(&json!({"path":file}), &paths, &Value::Null);
    assert_eq!(commit(&loose).as_deref(), Some(SHA));
    assert_eq!(commit(&packed).as_deref(), Some(OTHER));
}

#[test]
fn a_linked_work_tree_reads_its_own_head_and_the_shared_refs() {
    let (_dir, root) = workspace();
    work_tree(&root.join("main"), "ref: refs/heads/main");
    let linked_git = root.join("main/.git/worktrees/feature");
    fs::create_dir_all(&linked_git).expect("linked git dir");
    fs::write(linked_git.join("HEAD"), "ref: refs/heads/feature\n").expect("HEAD");
    fs::write(linked_git.join("commondir"), "../..\n").expect("commondir");
    fs::create_dir_all(root.join("main/.git/refs/heads")).expect("refs");
    fs::write(
        root.join("main/.git/refs/heads/feature"),
        format!("{OTHER}\n"),
    )
    .expect("ref");
    let tree = root.join("feature");
    fs::create_dir_all(tree.join("src")).expect("src");
    fs::write(
        tree.join(".git"),
        format!("gitdir: {}\n", linked_git.display()),
    )
    .expect(".git file");
    fs::write(tree.join("src/lib.rs"), "\n").expect("source");
    let paths = crate::test_support::workspace_policy(&root);
    let row = json!({"path": tree.join("src/lib.rs").to_string_lossy()});
    assert_eq!(
        checkout_commit(&row, &paths, &Value::Null).as_deref(),
        Some(OTHER)
    );
    // Its branch is its own; the remote is the repository's, shared.
    fs::write(
        root.join("main/.git/config"),
        "[remote \"origin\"]\n\turl = https://example.com/o/r.git\n",
    )
    .expect("config");
    let found = checkout(&row, &paths, &Value::Null).expect("checkout");
    assert_eq!(found.reference.as_deref(), Some("feature"));
    assert_eq!(found.remote.as_deref(), Some("example.com/o/r"));
}

#[test]
fn paths_in_several_work_trees_or_none_state_no_commit() {
    let (_dir, root) = workspace();
    let one = work_tree(&root.join("one"), SHA);
    let two = work_tree(&root.join("two"), OTHER);
    let loose = root.join("loose.rs");
    fs::write(&loose, "\n").expect("untracked file");
    let paths = crate::test_support::workspace_policy(&root);
    let row = json!({"files":[{"path":one},{"path":two}]});
    assert_eq!(checkout_commit(&row, &paths, &Value::Null), None);
    let outside = json!({"files":[{"path":one},{"path":loose.to_string_lossy()}]});
    assert_eq!(checkout_commit(&outside, &paths, &Value::Null), None);
    // What a walk skipped elsewhere is not evidence of the row.
    let skipped = json!({"files":[{"path":one}],"skippedPaths":{"entries":[{"path":two}]}});
    assert_eq!(
        checkout_commit(&skipped, &paths, &Value::Null).as_deref(),
        Some(SHA)
    );
}

#[test]
fn a_work_tree_above_the_allowed_roots_is_never_read() {
    let (_dir, outer) = workspace();
    let file = work_tree(&outer, SHA);
    // The workspace is a folder inside the outer work tree, without its own.
    let paths = crate::test_support::workspace_policy(&outer.join("src"));
    assert_eq!(
        checkout_commit(&json!({"path":file}), &paths, &Value::Null),
        None
    );
}

#[test]
fn a_head_that_climbs_out_of_refs_or_holds_no_object_id_states_nothing() {
    let (_dir, root) = workspace();
    let climbs = work_tree(&root.join("climbs"), "ref: refs/../../secret");
    fs::write(root.join("climbs/secret"), format!("{SHA}\n")).expect("file");
    let unborn = work_tree(&root.join("unborn"), "ref: refs/heads/main");
    let garbage = work_tree(&root.join("garbage"), "not an id");
    let paths = crate::test_support::workspace_policy(&root);
    for file in [climbs, unborn, garbage] {
        assert_eq!(
            checkout_commit(&json!({"path":file}), &paths, &Value::Null),
            None,
            "{file}"
        );
    }
}
