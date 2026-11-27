//! Paths: directories, policy refusals, missing files and their leads,
//! normal forms and symlinks.

use super::*;

/// A directory is `notAFile` and leads to its tree.
#[test]
fn directory_path_is_not_a_file_with_tree_lead() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src")).expect("dir");
    let mut req = test_local_fetch_query();
    local_fetch_query::set_path(&mut req, "src").expect("path");
    for result in [
        fetch(&req, &workspace_policy(&t.0)),
        fetch(&q(&t.0.join("src")), &Paths(t.0.clone())),
    ] {
        assert_eq!(result.error_code, Some(ErrorCode::NotAFile), "{result:?}");
        let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
        assert_eq!(next["viewTree"]["tool"], "structureSearch", "{next}");
        assert_eq!(
            next["viewTree"]["input"]["queries"][0]["operation"], "tree",
            "{next}"
        );
    }
}

/// A path the policy refuses keeps the policy's shared code, as every local
/// tool reports it: a NUL byte anywhere is malformed input (not a file to
/// look for), and a withheld path is denied.
#[test]
fn a_policy_refusal_keeps_the_shared_code() {
    let t = Fixture::new();
    fs::write(t.0.join("kv.json"), "{}\n").expect("file");
    fs::write(t.0.join(".env"), "A=1\n").expect("env");
    for (path, code) in [
        ("kv\u{0}.json", ErrorCode::InvalidInput),
        ("kv.json\u{0}", ErrorCode::InvalidInput),
        (".env", ErrorCode::PathPolicyDenied),
    ] {
        let mut req = test_local_fetch_query();
        local_fetch_query::set_path(&mut req, path).expect("path");
        let result = fetch(&req, &workspace_policy(&t.0));
        assert_eq!(result.error_code, Some(code), "{result:?}");
    }
}

/// The row names the file it read: `.` and `..` segments of the requested
/// path are resolved in the echoed `path`, the file's canonical path
/// relative to the workspace.
#[test]
fn the_row_path_is_the_normal_form_of_the_requested_one() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("sub")).expect("dir");
    let p = t.0.join("sub").join("a.rs");
    fs::write(&p, "fn a() {}\n").expect("fixture");
    let dotted =
        t.0.join("sub")
            .join("..")
            .join(".")
            .join("sub")
            .join("a.rs");
    let result = fetch(&qj(&dotted, serde_json::json!({})), &Paths(t.0.clone()));
    assert_eq!(result.status, ReadStatus::Success, "{result:?}");
    assert_eq!(result.path, "sub/a.rs", "{result:?}");
}

/// A `..` after a symlink leaves the link's target, not the link: the row
/// names the file the read resolved, never a lexical fold of the request.
#[cfg(unix)]
#[test]
fn the_row_path_follows_a_symlink_before_its_parent_segment() {
    let t = Fixture::new();
    let root = fs::canonicalize(&t.0).expect("canonical");
    fs::create_dir_all(root.join("other").join("inner")).expect("dir");
    fs::write(root.join("other").join("a.rs"), "fn other() {}\n").expect("fixture");
    fs::write(root.join("a.rs"), "fn lexical() {}\n").expect("fixture");
    std::os::unix::fs::symlink(root.join("other").join("inner"), root.join("link"))
        .expect("symlink");
    let result = fetch(
        &qj(Path::new("link/../a.rs"), serde_json::json!({})),
        &Paths(root.clone()),
    );
    assert_eq!(
        result.content.as_deref(),
        Some("fn other() {}\n"),
        "{result:?}"
    );
    assert_eq!(result.path, "other/a.rs", "{result:?}");
}

/// Whole-file and streamed reads through a symlink name the requested path
/// in the row and in every continuation, and the first page names the
/// file it resolved to; a path with no symlink names its canonical file.
#[cfg(unix)]
#[test]
fn reads_through_a_symlink_keep_the_requested_path() {
    let t = Fixture::new();
    let root = fs::canonicalize(&t.0).expect("canonical");
    fs::create_dir_all(root.join("other")).expect("dir");
    fs::write(root.join("other").join("a.rs"), "fn a() {}\n").expect("fixture");
    // Past the whole-file guard, the source is served from streamed windows.
    let line = format!("{}\n", "x".repeat(99));
    let lines = usize::try_from(acquire::MAX_SOURCE_BYTES).expect("size") / line.len() + 10;
    fs::write(root.join("other").join("big.txt"), line.repeat(lines)).expect("fixture");
    std::os::unix::fs::symlink(root.join("other"), root.join("link")).expect("symlink");
    std::os::unix::fs::symlink(root.join("other").join("a.rs"), root.join("file-link.rs"))
        .expect("symlink");
    let policy = workspace_policy(&root);
    for (requested, target, streamed) in [
        ("link/a.rs", Some("other/a.rs"), false),
        ("link/big.txt", Some("other/big.txt"), true),
        ("file-link.rs", Some("other/a.rs"), false),
        ("other/./a.rs", None, false),
    ] {
        let mut req = test_local_fetch_query();
        local_fetch_query::set_path(&mut req, requested).expect("path");
        let result = fetch(&req, &policy);
        assert_eq!(
            result.status,
            ReadStatus::Success,
            "{requested}: {:?}",
            result.error
        );
        assert_eq!(
            result
                .warnings
                .iter()
                .any(|w| w.starts_with("Large source")),
            streamed,
            "{requested}: {:?}",
            result.warnings
        );
        let disclosed = result
            .warnings
            .iter()
            .find_map(|w| w.split_once(" resolves through a symlink to "))
            .map(|(_, to)| to.trim_end_matches('.').to_owned());
        assert_eq!(disclosed.as_deref(), target, "{requested}");
        let named = if target.is_some() {
            requested
        } else {
            "other/a.rs"
        };
        assert_eq!(result.path, named, "{requested}");
        // A read that continues the file names the file its row names.
        let mut next = result.next.clone().unwrap_or_default();
        let reads = next
            .reads_mut()
            .map(|read| local_fetch_query::path(&read.query).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(!reads.is_empty(), streamed, "{requested}: {reads:?}");
        assert!(
            reads.iter().all(|path| *path == result.path),
            "{requested}: {reads:?}"
        );
        // The next page does not repeat where the link led.
        if let Some(read) = result
            .next
            .as_ref()
            .and_then(|next| next.continue_window.as_ref())
        {
            let page = fetch(&read.query, &policy);
            assert!(
                !page
                    .warnings
                    .iter()
                    .any(|w| w.contains("resolves through a symlink")),
                "{requested}: {:?}",
                page.warnings
            );
        }
    }
}

/// On a case-insensitive file system a path that differs from the file's
/// name only in letter case reads the file under its own name: no
/// component is a symlink, so no symlink is named.
#[test]
fn a_case_only_difference_is_not_a_symlink() {
    let t = Fixture::new();
    let root = fs::canonicalize(&t.0).expect("canonical");
    fs::create_dir_all(root.join("src")).expect("dir");
    fs::write(root.join("src/Readme.md"), "# readme\n").expect("fixture");
    if !root.join("SRC/README.MD").exists() {
        return; // case-sensitive: the other spelling names no file
    }
    let policy = workspace_policy(&root);
    let mut req = test_local_fetch_query();
    local_fetch_query::set_path(&mut req, "SRC/README.MD").expect("path");
    let result = fetch(&req, &policy);
    assert_eq!(result.status, ReadStatus::Success, "{:?}", result.error);
    assert!(
        !result.warnings.iter().any(|w| w.contains("symlink")),
        "{:?}",
        result.warnings
    );
    assert_eq!(result.path, "src/Readme.md", "{result:?}");
}

/// A missing file offers one call: the read of the one sibling its mistyped
/// name is close to, which runs; else its parent's tree, which shows what
/// is there.
#[test]
fn a_missing_file_offers_one_call_the_close_sibling_else_its_parents_tree() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src")).expect("dir");
    fs::write(t.0.join("src/executor.rs"), "fn a() {}\n").expect("fixture");
    let policy = workspace_policy(&t.0);
    let missing = |name: &str| {
        let r = fetch(&qj(&t.0.join(name), serde_json::json!({})), &policy);
        assert_eq!(r.error_code, Some(ErrorCode::NotFound), "{r:?}");
        assert!(!r.path.is_empty(), "an error row names its path");
        let next = serde_json::to_value(r.next.as_ref().expect("next")).expect("json");
        assert_eq!(
            octocode_runtime_base::test_support::billing::offered_calls(&next).len(),
            1,
            "{name}: one call: {next}"
        );
        (r, next)
    };
    let (mistyped, next) = missing("src/exector.rs");
    let read = &mistyped
        .next
        .as_ref()
        .expect("next")
        .read
        .as_ref()
        .expect("read")
        .query;
    assert_eq!(
        local_fetch_query::path(read),
        t.0.join("src/executor.rs").to_string_lossy(),
        "{next}"
    );
    assert_eq!(fetch(read, &policy).status, ReadStatus::Success);
    let (_, next) = missing("src/zzqqxx.md");
    let tree = &next["viewTree"]["input"]["queries"][0];
    assert_eq!(tree["operation"], "tree", "{next}");
    assert_eq!(tree["path"], "src", "{next}");
}

/// A whole listing entry given as the path says so and offers one call,
/// the read of the entry's own path, which runs; a file whose real name
/// ends in `(…)` opens.
#[test]
fn a_listing_entry_given_as_a_path_offers_the_read_of_its_path() {
    use octocode_tools_shared::root_read::{entry_text, listing_entry_note};
    let t = Fixture::new();
    t.write("src/big.ts", "x\n");
    t.write("src/notes (old)", "y\n");
    let policy = workspace_policy(&t.0);
    let entry = entry_text("big.ts", false, &["700".to_owned()]);
    let r = fetch(
        &qj(&t.0.join("src").join(entry), serde_json::json!({})),
        &policy,
    );
    assert_eq!(r.error_code, Some(ErrorCode::NotFound), "{r:?}");
    let next = serde_json::to_value(r.next.as_ref().expect("next")).expect("json");
    assert_eq!(
        octocode_runtime_base::test_support::billing::offered_calls(&next).len(),
        1,
        "{next}"
    );
    let read = &r
        .next
        .as_ref()
        .expect("next")
        .read
        .as_ref()
        .expect("read")
        .query;
    let bare = local_fetch_query::path(read);
    assert_eq!(bare, t.0.join("src/big.ts").to_string_lossy());
    assert!(
        r.error
            .as_deref()
            .is_some_and(|error| error.ends_with(&listing_entry_note(bare))),
        "{r:?}"
    );
    assert_eq!(fetch(read, &policy).status, ReadStatus::Success);
    let real = fetch(
        &qj(&t.0.join("src/notes (old)"), serde_json::json!({})),
        &policy,
    );
    assert_eq!(real.status, ReadStatus::Success, "{real:?}");
}

/// A mistyped name close to a file the path policy withholds names no
/// read of it: recovery never reveals a withheld file.
#[test]
fn a_missing_file_close_to_a_withheld_one_offers_no_read() {
    let t = Fixture::new();
    t.write(".aws/credentials", "secret\n");
    let policy = workspace_policy(&t.0);
    let r = fetch(
        &qj(&t.0.join(".aws/credentails"), serde_json::json!({})),
        &policy,
    );
    assert!(r.error_code.is_some(), "{r:?}");
    assert!(
        r.next.as_ref().is_none_or(|next| next.read.is_none()),
        "{:?}",
        r.next
    );
}

/// A kubeconfig under a name a person gives it (`*.kubeconfig`) is refused
/// like `.kube/config`, and a Go source file named for the word stays
/// readable (round 12).
#[test]
fn a_renamed_kubeconfig_is_refused_and_its_source_stays_readable() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("kube")).expect("dir");
    fs::write(
        t.0.join("kube/prod.kubeconfig"),
        "apiVersion: v1\nkind: Config\nusers:\n- name: admin\n  user:\n    token: s3cr3t\n",
    )
    .expect("kubeconfig");
    fs::write(t.0.join("kube/kubeconfig.go"), "package kube\n").expect("source");
    for (path, code) in [
        ("kube/prod.kubeconfig", Some(ErrorCode::PathPolicyDenied)),
        ("kube/kubeconfig.go", None),
    ] {
        let mut req = test_local_fetch_query();
        local_fetch_query::set_path(&mut req, path).expect("path");
        let result = fetch(&req, &workspace_policy(&t.0));
        assert_eq!(result.error_code, code, "{path}: {result:?}");
    }
}

/// A link to a withheld file is refused without naming the file it points to:
/// the caller asked for the link, not for the credential's name (RV-SEC round
/// 12 review).
#[cfg(unix)]
#[test]
fn a_link_to_a_credential_is_refused_without_naming_its_target() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("k")).expect("dir");
    fs::write(t.0.join("k/prod.kubeconfig"), "kind: Config\n").expect("kubeconfig");
    std::os::unix::fs::symlink(t.0.join("k/prod.kubeconfig"), t.0.join("k/notes.yaml"))
        .expect("link");
    let mut req = test_local_fetch_query();
    local_fetch_query::set_path(&mut req, "k/notes.yaml").expect("path");
    let result = fetch(&req, &workspace_policy(&t.0));
    assert_eq!(
        result.error_code,
        Some(ErrorCode::PathPolicyDenied),
        "{result:?}"
    );
    let shown = format!("{result:?}");
    assert!(!shown.contains("prod.kubeconfig"), "{shown}");
    assert!(shown.contains("a credential file"), "{shown}");
}
