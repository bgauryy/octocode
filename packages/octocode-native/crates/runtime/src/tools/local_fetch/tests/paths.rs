//! Paths: directories, policy refusals, missing files and their leads,
//! normal forms and symlinks.

use super::*;

/// A directory is `notAFile` and leads to its tree.
#[test]
fn directory_path_is_not_a_file_with_tree_lead() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src")).expect("dir");
    let mut req = test_local_fetch_query();
    req.path = "src".parse().expect("path");
    for result in [
        fetch(&req, &workspace_policy(&t.0)),
        fetch(&q(&t.0.join("src")), &Paths(t.0.clone())),
    ] {
        assert_eq!(result.error_code, Some(ErrorCode::NotAFile), "{result:?}");
        let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
        assert_eq!(next["viewTree"]["tool"], "structureSearch", "{next}");
        assert_eq!(
            next["viewTree"]["query"]["queries"][0]["operation"], "tree",
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
        req.path = path.parse().expect("path");
        let result = fetch(&req, &workspace_policy(&t.0));
        assert_eq!(result.error_code, Some(code), "{result:?}");
    }
}

/// A missing file leads to a listing by its stem from the workspace root:
/// a file moved to another tree (or a path that names the wrong package)
/// is found at any depth, not only under the closest existing directory.
#[test]
fn missing_file_leads_to_files_by_stem() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src/a")).expect("dirs");
    fs::create_dir_all(t.0.join("pkg/docs")).expect("dirs");
    fs::write(t.0.join("pkg/docs/blok.ts"), "x\n").expect("moved");
    let mut req = test_local_fetch_query();
    req.path = "src/a/blok.rs".parse().expect("path");
    let result = fetch(&req, &workspace_policy(&t.0));
    assert_eq!(
        result.error_code,
        Some(ErrorCode::PathNotFound),
        "{result:?}"
    );
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let listing = &next["findFile"]["query"]["queries"][0];
    assert_eq!(listing["operation"], "files", "{next}");
    assert_eq!(listing["path"], ".", "{next}");
    assert_eq!(listing["include"], serde_json::json!(["blok.*"]), "{next}");
    // The lead runs and lists the moved file.
    let (paths, security) = crate::tools::test_support::simple_policy(&t.0);
    let listed = crate::tools::structure_search::execute_structure(
        &serde_json::from_value(listing.clone()).expect("structureSearch row"),
        &paths,
        &security,
        &crate::execution::cancel::NeverCancel,
        None,
    )
    .expect("listing");
    assert!(
        listed["files"]
            .as_array()
            .is_some_and(|dirs| dirs.iter().any(|dir| {
                dir["dir"] == "pkg/docs"
                    && dir["files"].as_array().is_some_and(|names| {
                        names
                            .iter()
                            .any(|name| name.as_str().is_some_and(|n| n.starts_with("blok.ts")))
                    })
            })),
        "{listed}"
    );
}

/// The lead keeps the whole stem as a glob, so a short name (`x.md`)
/// lists only `x.*`, never every name that contains an `x`.
#[test]
fn missing_file_lead_never_widens_to_names_containing_the_stem() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("docs")).expect("dirs");
    for name in ["x.txt", "box.md", "index.md"] {
        fs::write(t.0.join("docs").join(name), "x\n").expect("fixture");
    }
    let mut req = test_local_fetch_query();
    req.path = "docs/x.md".parse().expect("path");
    let result = fetch(&req, &workspace_policy(&t.0));
    let next = serde_json::to_value(result.next.as_ref().expect("next")).expect("json");
    let listing = &next["findFile"]["query"]["queries"][0];
    assert_eq!(listing["include"], serde_json::json!(["x.*"]), "{next}");
    let globs = crate::policy::include::include_globs(&["x.*".to_owned()]);
    assert_eq!(globs, vec!["x.*".to_owned()], "a glob is not widened");
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
        req.path = requested.parse().expect("path");
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
            .map(|read| read.query.path.to_string())
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
    req.path = "SRC/README.MD".parse().expect("path");
    let result = fetch(&req, &policy);
    assert_eq!(result.status, ReadStatus::Success, "{:?}", result.error);
    assert!(
        !result.warnings.iter().any(|w| w.contains("symlink")),
        "{:?}",
        result.warnings
    );
    assert_eq!(result.path, "src/Readme.md", "{result:?}");
}

/// A missing file leads to its parent's tree, where a mistyped name's
/// siblings show, beside the listing that finds a moved file and the read
/// of the one sibling its mistyped name is close to.
#[test]
fn a_missing_file_leads_to_its_parents_tree() {
    let t = Fixture::new();
    fs::create_dir_all(t.0.join("src")).expect("dir");
    fs::write(t.0.join("src/executor.rs"), "fn a() {}\n").expect("fixture");
    let policy = workspace_policy(&t.0);
    let r = fetch(
        &qj(&t.0.join("src/exector.rs"), serde_json::json!({})),
        &policy,
    );
    assert_eq!(r.error_code, Some(ErrorCode::PathNotFound), "{r:?}");
    let next = serde_json::to_value(r.next.as_ref().expect("next")).expect("json");
    let tree = &next["viewTree"]["query"]["queries"][0];
    assert_eq!(tree["operation"], "tree", "{next}");
    assert_eq!(tree["path"], "src", "{next}");
    assert!(next.get("findFile").is_some(), "{next}");
    assert!(!r.path.is_empty(), "an error row names its path");
    let read = &r
        .next
        .as_ref()
        .expect("next")
        .read
        .as_ref()
        .expect("read")
        .query;
    assert_eq!(
        read.path.as_str(),
        t.0.join("src/executor.rs").to_string_lossy(),
        "{next}"
    );
    assert_eq!(fetch(read, &policy).status, ReadStatus::Success);
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
